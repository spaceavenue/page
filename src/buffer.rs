use std::fs::File;
use std::io::{self, Read};
use std::ops::Deref;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread;

use anyhow::Context;
use memchr::memchr_iter;
use memmap2::{Mmap, MmapOptions};

use crate::config::cli::Input;

/// Files below ~64KiB are faster to load using `read()` instead of `mmap()`-ing them.
const MMAP_THRESHOLD: usize = 64 * 1024; // 64 KiB

/// Read stdin in growing chunks to minimize syscall overhead.
const STDIN_MIN_CHUNK: usize = 8 * 1024;
const STDIN_MAX_CHUNK: usize = 256 * 1024;

/// How much to stream ahead of current line before blocking the pipe.
const STDIN_CHANNEL_CAPACITY: usize = 32;

enum BufferSource {
  Mmap(Mmap),
  Heap(Vec<u8>),
}

impl Deref for BufferSource {
  type Target = [u8];

  fn deref(&self) -> &Self::Target {
    match self {
      Self::Mmap(mmap) => mmap,
      Self::Heap(heap) => heap,
    }
  }
}

pub struct Buffer {
  source: BufferSource,
  line_offsets: Vec<usize>,
  scanned: usize,
  filename: String,
  stdin_rx: Option<Receiver<io::Result<Vec<u8>>>>,
}

impl Buffer {
  /// Load a buffer from either a file path or stdin.
  ///
  /// # Errors
  ///
  /// Returns an error if reading from the file or configuring stdin fails.
  pub fn load(input: &Input) -> anyhow::Result<Self> {
    match input {
      Input::Path(path) => {
        Self::from_file(path).with_context(|| format!("failed to read file: {}", path.display()))
      }
      Input::Stdin => Self::from_stdin(),
    }
  }

  /// Load a file via Mmap.
  ///
  /// # Errors
  ///
  /// Returns an error if opening or reading the file fails.
  pub fn from_file(path: &Path) -> anyhow::Result<Self> {
    let mut file = File::open(path)?;
    let size = usize::try_from(file.metadata()?.len())
      .context("file is too large to fit in memory address space.")?;

    let source = if size < MMAP_THRESHOLD {
      let mut buf = Vec::with_capacity(size);
      file.read_to_end(&mut buf)?;
      BufferSource::Heap(buf)
    } else {
      let mmap = unsafe { MmapOptions::new().len(size).map(&file)? };
      // hint the kernel to perform bulk readahead and sequential TLB prep
      let _ = mmap.advise(memmap2::Advice::Sequential);
      BufferSource::Mmap(mmap)
    };

    let filename = path
      .file_name()
      .and_then(|f| f.to_str())
      .unwrap_or("file")
      .to_string();

    Ok(Self {
      source,
      line_offsets: Vec::new(),
      scanned: 0,
      filename,
      stdin_rx: None,
    })
  }

  /// Load bytes from stdin.
  ///
  /// # Errors
  ///
  /// Returns an error if spawning the stdin reader thread fails.
  pub fn from_stdin() -> anyhow::Result<Self> {
    let (tx, rx) = mpsc::sync_channel(STDIN_CHANNEL_CAPACITY);
    Self::spawn_stdin_reader(tx)?;

    Ok(Self {
      source: BufferSource::Heap(Vec::with_capacity(STDIN_MIN_CHUNK * 4)),
      line_offsets: Vec::new(),
      scanned: 0,
      filename: "[stdin]".to_string(),
      stdin_rx: Some(rx),
    })
  }

  fn spawn_stdin_reader(tx: SyncSender<io::Result<Vec<u8>>>) -> anyhow::Result<()> {
    thread::Builder::new()
      .name("stdin_reader".into())
      .spawn(move || {
        let mut stdin = io::stdin().lock();
        let mut chunk_size = STDIN_MIN_CHUNK;
        loop {
          let mut chunk = vec![0u8; chunk_size];
          match stdin.read(&mut chunk) {
            Ok(0) => break, // EOF
            Ok(n) => {
              chunk.truncate(n);
              if tx.send(Ok(chunk)).is_err() {
                break;
              }
              chunk_size = (chunk_size * 2).min(STDIN_MAX_CHUNK);
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => {
              let _ = tx.send(Err(e));
              break;
            }
          }
        }
      })
      .context("failed to spawn stdin reader thread")?;

    Ok(())
  }

  /// Read newly arrived data from the background stdin reader.
  ///
  /// # Errors
  ///
  /// Returns an error if an I/O error occurred while reading from stdin.
  pub fn read_from_stdin(&mut self) -> anyhow::Result<bool> {
    let Some(rx) = self.stdin_rx.as_ref() else {
      return Ok(false);
    };

    let mut received = false;
    loop {
      match rx.try_recv() {
        Ok(Ok(chunk)) => {
          if let BufferSource::Heap(buf) = &mut self.source {
            buf.extend_from_slice(&chunk);
          }
          received = true;
        }
        Ok(Err(e)) => {
          self.stdin_rx = None;
          return Err(e).context("error reading from stdin");
        }
        Err(mpsc::TryRecvError::Empty) => break,
        Err(mpsc::TryRecvError::Disconnected) => {
          self.stdin_rx = None;
          break;
        }
      }
    }

    Ok(received)
  }

  #[must_use]
  pub const fn is_streaming(&self) -> bool {
    self.stdin_rx.is_some()
  }

  /// Whether every byte currently available has been scanned for newlines.
  ///
  /// A streaming source is never fully indexed until its reader disconnects,
  /// even if all data received so far has been scanned.
  #[must_use]
  pub fn is_fully_indexed(&self) -> bool {
    !self.is_streaming() && self.scanned >= self.source.len()
  }

  pub fn index_till_line(&mut self, target_line: usize) {
    if self.line_offsets.len() > target_line {
      return;
    }

    let bytes = &self.source;
    let start = self.scanned;
    for pos in memchr_iter(b'\n', bytes.get(start..).unwrap_or(&[])) {
      self.line_offsets.push(start + pos);
      if self.line_offsets.len() > target_line {
        self.scanned = start + pos + 1;
        return;
      }
    }

    self.scanned = bytes.len();
  }

  #[must_use]
  pub fn filename(&self) -> &str {
    &self.filename
  }

  #[must_use]
  pub fn line_count(&self) -> usize {
    let tail_start = self.line_offsets.last().map_or(0, |&newline| newline + 1);
    let unterminated_tail = self.is_fully_indexed() && tail_start < self.source.len();
    self.line_offsets.len() + usize::from(unterminated_tail)
  }

  #[must_use]
  pub fn line_range(&self, line: usize) -> Option<(usize, usize)> {
    let start = match line.checked_sub(1) {
      Some(prev) => self.line_offsets.get(prev)? + 1,
      None => 0,
    };

    let len = self.source.len();
    let end = match self.line_offsets.get(line) {
      Some(&newline) => newline,
      None if self.is_fully_indexed() && start < len => len,
      None => return None,
    };

    Some((start, end))
  }

  #[must_use]
  pub fn line_bytes(&self, line: usize) -> Option<&[u8]> {
    let (start, end) = self.line_range(line)?;
    self.source.get(start..end)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn buffer_from(contents: &[u8]) -> Buffer {
    let contents = contents.to_vec();
    Buffer {
      source: BufferSource::Heap(contents),
      line_offsets: Vec::new(),
      scanned: 0,
      filename: "test".into(),
      stdin_rx: None,
    }
  }

  #[test]
  fn indexing_through_line_makes_it_readable() {
    let mut buf = buffer_from(b"hello\nworld\n");
    buf.index_till_line(1);
    assert_eq!(buf.line_bytes(0), Some(&b"hello"[..]));
    assert_eq!(buf.line_bytes(1), Some(&b"world"[..]));
    assert_eq!(buf.line_bytes(2), None);
    assert_eq!(buf.line_count(), 2);
  }

  #[test]
  fn unterminated_last_line_readable() {
    let mut buf = buffer_from(b"a\nb\nc");
    buf.index_till_line(2);
    assert_eq!(buf.line_bytes(0), Some(&b"a"[..]));
    assert_eq!(buf.line_bytes(1), Some(&b"b"[..]));
    assert_eq!(buf.line_bytes(2), Some(&b"c"[..]));
    assert_eq!(buf.line_count(), 3);
  }
}
