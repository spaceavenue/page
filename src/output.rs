use std::fs::File;
use std::io::ErrorKind::BrokenPipe;
use std::io::{self, Write};

use crate::buffer::Buffer;
use crate::config::cli::Input;
use crate::config::options::Options;
use crate::text::{ParsedLine, WrapMode};
use crate::ui::viewport::Viewport;

pub fn passthrough(input: &Input, out: &mut impl Write) -> io::Result<u64> {
  let res = match input {
    Input::Stdin => io::copy(&mut io::stdin().lock(), out),
    Input::Path(path_buf) => {
      let mut file = File::open(path_buf)?;
      io::copy(&mut file, out)
    },
  };
  match res {
    Ok(n) => Ok(n),
    Err(e) if e.kind() == BrokenPipe => Ok(0),
    Err(e) => Err(e),
  }
}

pub fn fits(buffer: &mut Buffer, opts: &Options, viewport: Viewport) -> bool {
  let body_height = viewport.body_height();

  buffer.index_till_line(body_height);
  if !buffer.is_fully_indexed() || buffer.line_count() > body_height {
    return false;
  }
  if opts.wrap == WrapMode::None {
    return true;
  }
  let gutter = viewport.gutter_width(opts.line_numbers, buffer.line_count());
  let width = viewport.content_width(gutter);
  let mut parse_buf = ParsedLine::default();
  let mut acc = 0usize;
  for line in 0..buffer.line_count() {
    let raw = buffer.line_bytes(line).unwrap_or_default();
    parse_buf.parse(raw, opts.ansi, opts.tab_width);
    acc += parse_buf.count_rows(width, opts.wrap);
    if acc > body_height {
      return false;
    }
  }
  true
}
