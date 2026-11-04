use std::fs::File;
use std::io::{self, Read};
use std::ops::{Deref, Range};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread;
use std::time::Duration;

use anyhow::Context;
use memchr::memchr_iter;
use memmap2::{Mmap, MmapOptions};

use crate::config::cli::Input;

/// Files below ~64KiB are faster to load using `read()` instead of `mmap()`-ing them.
const MMAP_THRESHOLD: usize = 64 * 1024; // 64 KiB

/// Read stdin in chunks of 64KiB.
const STDIN_CHUNK_SIZE: usize = 64 * 1024;

/// How much to stream ahead of current line before blocking the pipe.
const STDIN_CHANNEL_CAPACITY: usize = 32;

enum BufferSource {
  Mmap(Mmap),
  Heap(Vec<u8>),
  Stream(StreamBuffer),
}

struct StdinStream {
  data_rx: Receiver<io::Result<Vec<u8>>>,
  vec_tx: SyncSender<Vec<u8>>,
}

struct StreamBuffer {
  data: Vec<u8>,
  stream: Option<StdinStream>,
}

impl StreamBuffer {
  const fn is_streaming(&self) -> bool {
    self.stream.is_some()
  }

  fn read_chunk(&mut self) -> anyhow::Result<bool> {
    let Some(stream) = self.stream.as_mut() else {
      return Ok(false);
    };

    let mut received = false;
    loop {
      match stream.data_rx.try_recv() {
        Ok(Ok(mut chunk)) => {
          self.data.extend_from_slice(&chunk);
          received = true;
          chunk.clear();
          let _ = stream.vec_tx.try_send(chunk);
        },
        Ok(Err(e)) => {
          self.stream = None;
          return Err(e).context("error reading from stdin");
        },
        Err(mpsc::TryRecvError::Empty) => break,
        Err(mpsc::TryRecvError::Disconnected) => {
          self.stream = None;
          break;
        },
      }
    }

    Ok(received)
  }
}

impl Deref for BufferSource {
  type Target = [u8];

  fn deref(&self) -> &Self::Target {
    match self {
      Self::Mmap(mmap) => mmap,
      Self::Heap(heap) => heap,
      Self::Stream(stream) => &stream.data,
    }
  }
}

pub struct Buffer {
  source: BufferSource,
  line_offsets: Vec<usize>,
  scanned: usize,
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
      },
      Input::Stdin => Self::from_stdin(),
    }
  }

  /// Load a file via Mmap.
  ///
  /// # Errors
  ///
  /// Returns an error if opening or reading the file fails.
  fn from_file(path: &Path) -> anyhow::Result<Self> {
    let mut file = File::open(path)?;
    let size = usize::try_from(file.metadata()?.len())
      .context("file is too large to fit in memory address space.")?;

    let source = if size < MMAP_THRESHOLD {
      let mut buf = Vec::with_capacity(size);
      file.read_to_end(&mut buf)?;
      BufferSource::Heap(buf)
    } else {
      // SAFETY: if a file is truncated while open, we will receive a SIGBUS.
      let mmap = unsafe { MmapOptions::new().len(size).map(&file)? };
      // hint the kernel to perform bulk readahead and sequential TLB prep
      let _ = mmap.advise(memmap2::Advice::Sequential);
      BufferSource::Mmap(mmap)
    };

    Ok(Self {
      source,
      line_offsets: Vec::new(),
      scanned: 0,
    })
  }

  /// Load bytes from stdin.
  ///
  /// # Errors
  ///
  /// Returns an error if spawning the stdin reader thread fails.
  fn from_stdin() -> anyhow::Result<Self> {
    let (data_tx, data_rx) = mpsc::sync_channel(STDIN_CHANNEL_CAPACITY);
    let (vec_tx, vec_rx) = mpsc::sync_channel(STDIN_CHANNEL_CAPACITY);
    Self::spawn_stdin_reader(data_tx, vec_rx)?;

    let mut data = Vec::with_capacity(STDIN_CHUNK_SIZE * 4);
    let mut active = true;

    match data_rx.recv_timeout(Duration::from_millis(10)) {
      Ok(Ok(mut chunk)) => {
        data.extend_from_slice(&chunk);
        chunk.clear();
        let _ = vec_tx.try_send(chunk);
      },
      Ok(Err(e)) => return Err(e).context("error reading from stdin"),
      Err(mpsc::RecvTimeoutError::Disconnected) => active = false,
      Err(mpsc::RecvTimeoutError::Timeout) => {},
    }

    let stream = active.then_some(StdinStream { data_rx, vec_tx });

    Ok(Self {
      source: BufferSource::Stream(StreamBuffer { data, stream }),
      line_offsets: Vec::new(),
      scanned: 0,
    })
  }

  fn spawn_stdin_reader(
    data_tx: SyncSender<io::Result<Vec<u8>>>,
    vec_rx: Receiver<Vec<u8>>,
  ) -> anyhow::Result<()> {
    thread::Builder::new()
      .name("stdin_reader".into())
      .spawn(move || {
        let mut stdin = io::stdin().lock();
        loop {
          let mut chunk = vec_rx.try_recv().unwrap_or_default();
          chunk.resize(STDIN_CHUNK_SIZE, 0);
          match stdin.read(&mut chunk) {
            Ok(0) => break, // EOF
            Ok(n) => {
              chunk.truncate(n);
              if data_tx.send(Ok(chunk)).is_err() {
                break;
              }
            },
            Err(e) if matches!(e.kind(), io::ErrorKind::Interrupted) => {},
            Err(e) => {
              let _ = data_tx.send(Err(e));
              break;
            },
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
    let BufferSource::Stream(stream) = &mut self.source else {
      return Ok(false);
    };
    stream.read_chunk()
  }

  /// Check if buffer is streaming data from stdin.
  #[must_use]
  pub const fn is_streaming(&self) -> bool {
    matches!(&self.source, BufferSource::Stream(stream) if stream.is_streaming())
  }

  /// Whether every byte currently available has been scanned for newlines.
  ///
  /// A streaming source is never fully indexed until its reader disconnects,
  /// even if all data received so far has been scanned.
  #[must_use]
  pub fn is_fully_indexed(&self) -> bool {
    !self.is_streaming() && self.scanned >= self.source.len()
  }

  /// Index till at least the `target` line.
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

  /// Get total number of lines indedex so far.
  #[must_use]
  pub fn line_count(&self) -> usize {
    let tail_start = self.line_offsets.last().map_or(0, |&newline| newline + 1);
    let unterminated_tail = self.is_fully_indexed() && tail_start < self.source.len();
    self.line_offsets.len() + usize::from(unterminated_tail)
  }

  /// Get the start and end byte offsets for `line`.
  #[must_use]
  pub fn line_range(&self, line: usize) -> Option<Range<usize>> {
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

    Some(start..end)
  }

  /// Get the bytes for `line`.
  #[must_use]
  pub fn line_bytes(&self, line: usize) -> Option<&[u8]> {
    let line_range = self.line_range(line)?;
    self.source.get(line_range)
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

  #[test]
  #[allow(clippy::unwrap_used)]
  fn stream_source_indexing_and_reading() {
    let (data_tx, data_rx) = mpsc::sync_channel(2);
    let (vec_tx, vec_rx) = mpsc::sync_channel(2);
    let mut buf = Buffer {
      source: BufferSource::Stream(StreamBuffer {
        data: Vec::new(),
        stream: Some(StdinStream { data_rx, vec_tx }),
      }),
      line_offsets: Vec::new(),
      scanned: 0,
    };

    assert!(buf.is_streaming());
    assert!(!buf.is_fully_indexed());

    data_tx.send(Ok(b"line 1\nline 2\n".to_vec())).unwrap();
    assert!(buf.read_from_stdin().unwrap());
    buf.index_till_line(1);
    assert_eq!(buf.line_bytes(0), Some(&b"line 1"[..]));
    assert_eq!(buf.line_bytes(1), Some(&b"line 2"[..]));

    // Recycled vector should be received back in vec_rx
    let recycled = vec_rx.try_recv().unwrap();
    assert_eq!(recycled.len(), 0);

    drop(data_tx);
    // EOF disconnect
    assert!(!buf.read_from_stdin().unwrap());
    assert!(!buf.is_streaming());
    assert!(buf.is_fully_indexed());
    assert_eq!(buf.line_count(), 2);
  }
}
