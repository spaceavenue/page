pub mod ansi;
pub mod reader;
mod render;
mod wrap;

use std::num::NonZeroUsize;
use std::ops::Range;

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::text::ansi::Style;
use crate::text::reader::Reader;

const ESC: char = '\x1b';
const DEL: char = '\x7f';
/// Caret notation shows control byte `b` as `^` followed by `b + 0x40`.
const CARET_OFFSET: u8 = 0x40;

/// How lines wider than the terminal are displayed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WrapMode {
  /// Break at word boundaries.
  #[default]
  Soft,
  /// Break at the column limit.
  Hard,
  /// Do not wrap.
  None,
}

/// A styled run in a parsed line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Run {
  /// Byte offset into [`ParsedLine::text`] where this run starts.
  start: usize,
  style: Style,
}

impl Run {
  /// Start offset of this run in the line's plain text. Test-only.
  #[must_use]
  pub const fn start(&self) -> usize {
    self.start
  }

  /// Visual style of this run. Test-only.
  #[must_use]
  pub const fn style(&self) -> Style {
    self.style
  }
}

/// A single logical line of parsed input.
///
/// `text` has no escape sequences, expanded tabs and control bytes rendered as caret notation.
#[derive(Debug, Default)]
pub struct ParsedLine {
  text: String,
  runs: Vec<Run>,
}

impl ParsedLine {
  /// Parse `raw` into printable text and expand tabs to `tab_width`.
  pub fn parse(&mut self, raw: &[u8], ansi_enabled: bool, tab_width: NonZeroUsize) {
    self.text.clear();
    self.runs.clear();

    let source = String::from_utf8_lossy(raw);
    let source = source.strip_suffix('\n').unwrap_or(&source);
    let source = source.strip_suffix('\r').unwrap_or(source);

    let mut style = Style::default();
    let mut run_start = 0usize;
    let mut column = 0usize;
    let mut reader = Reader::new(source);

    while let Some(ch) = reader.peek() {
      match ch {
        ESC => {
          let next_style = ansi::escape(&mut reader, ansi_enabled, style);

          if next_style != style {
            self.flush_run(&mut run_start, style);
            style = next_style;
          }
        }
        '\t' => {
          reader.bump();
          let spaces = tab_width.get() - (column % tab_width);
          self.text.extend(std::iter::repeat_n(' ', spaces));
          column += spaces;
        }
        // Lines never normally contain a newline, but be defensive.
        '\n' => {
          reader.bump();
        }
        // Printable ASCII is one column per byte, so copy whole runs at once.
        ' '..='~' => {
          let run = reader.read_while(|c| matches!(c, ' '..='~'));
          self.text.push_str(run);
          column += run.len();
        }
        _ if ch.is_ascii_control() => {
          reader.bump();
          self.text.push('^');
          self.text.push(caret(ch));
          column += 2;
        }
        // C1 controls are valid UTF-8 but some terminals still interpret them as escape
        // introducers.
        _ if ch.is_control() => {
          reader.bump();
          self.text.push('\u{fffd}');
          column += 1;
        }
        _ => {
          reader.bump();
          self.text.push(ch);
          column += UnicodeWidthChar::width(ch).unwrap_or(0);
        }
      }
    }

    self.flush_run(&mut run_start, style);
  }

  fn flush_run(&mut self, run_start: &mut usize, style: Style) {
    if *run_start >= self.text.len() {
      return;
    }
    self.runs.push(Run {
      start: *run_start,
      style,
    });
    *run_start = self.text.len();
  }

  /// The plain text of this parsed line.
  #[must_use]
  pub fn text(&self) -> &str {
    &self.text
  }

  /// Style runs.
  #[must_use]
  pub fn runs(&self) -> &[Run] {
    &self.runs
  }

  /// Lazily lay out the display rows of this line as byte ranges into [`ParsedLine::text`].
  pub fn rows(&self, width: usize, word_wrap: bool) -> impl Iterator<Item = Range<usize>> + '_ {
    wrap::row_ranges(&self.text, width, word_wrap)
  }

  /// Count display rows without retaining the row ranges.
  #[must_use]
  pub fn count_rows(&self, width: usize, word_wrap: bool) -> usize {
    self.rows(width, word_wrap).count()
  }

  /// Width of this line in terminal columns.
  #[must_use]
  pub fn display_width(&self) -> usize {
    UnicodeWidthStr::width(self.text.as_str())
  }

  /// Byte range covering the display columns `[left, left + width)`.
  ///
  /// A wide character that straddles the left edge is skipped rather than split. If `left` is past
  /// the end of the line the returned range is empty.
  #[must_use]
  pub fn column_range(&self, left: usize, width: usize) -> Range<usize> {
    if width == 0 || self.text.is_empty() {
      let end = self.text.len();
      return end..end;
    }

    let limit = left.saturating_add(width);
    let mut column = 0usize;
    let mut start = None;
    let mut end = 0usize;

    for (index, ch) in self.text.char_indices() {
      let char_width = UnicodeWidthChar::width(ch).unwrap_or(0);
      let next = column + char_width;

      if char_width == 0 {
        if start.is_some() {
          end = index + ch.len_utf8();
        }
        column = next;
        continue;
      }

      if next <= left {
        column = next;
        continue;
      }
      if column < left {
        // A wide character straddles the left edge.
        column = next;
        continue;
      }
      if column >= limit || next > limit {
        break;
      }

      start.get_or_insert(index);
      end = index + ch.len_utf8();
      column = next;
    }

    start.map_or(self.text.len()..self.text.len(), |start| start..end)
  }
}

fn caret(ch: char) -> char {
  if ch == DEL {
    return '?';
  }
  u8::try_from(ch).map_or('\u{fffd}', |byte| char::from(byte + CARET_OFFSET))
}

#[cfg(test)]
mod tests;
