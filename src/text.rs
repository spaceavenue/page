pub mod ansi;
pub mod reader;
mod render;
mod wrap;

use std::num::NonZeroUsize;
use std::ops::Range;

use crossterm::style::Attribute;
use memchr::memchr;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

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

/// A horizontal viewport slice into a parsed line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColumnWindow {
  /// Blank columns to emit before [`ColumnWindow::range`].
  pub pad: usize,
  /// Byte range of the visible glyphs.
  pub range: Range<usize>,
}

/// A styled run in a parsed line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Run {
  /// Byte offset into [`ParsedLine::text`] where this run starts.
  start: usize,
  style: Style,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Highlight {
  /// Range of matched text.
  pub(crate) range: Range<usize>,
  pub(crate) style: Style,
}

struct LineBuilder<'a> {
  line: &'a mut ParsedLine,
  base_style: Style,
  run_style: Style,
  run_start: usize,
  column: usize,
}

impl<'a> LineBuilder<'a> {
  fn new(line: &'a mut ParsedLine) -> Self {
    Self {
      line,
      base_style: Style::default(),
      run_style: Style::default(),
      run_start: 0,
      column: 0,
    }
  }

  fn parse_text(&mut self, source: &str, ansi_enabled: bool, tab_width: NonZeroUsize) {
    let mut reader = Reader::new(source);
    while let Some(ch) = reader.peek() {
      match ch {
        ESC => self.base_style = ansi::escape(&mut reader, ansi_enabled, self.base_style),
        '\t' => {
          reader.bump();
          self.push_tab(tab_width);
        },
        // Lines never normally contain a newline, but be defensive.
        '\n' => {
          reader.bump();
        },
        // Printable ASCII is one column per byte, so copy whole runs at once.
        ' '..='~' => {
          let run = reader.read_while(|c| matches!(c, ' '..='~'));
          self.push_run(run);
        },
        _ if ch.is_ascii_control() => {
          reader.bump();
          self.push_caret(ch);
        },
        // C1 controls are valid UTF-8 but some terminals still interpret them
        // as escape introducers.
        _ if ch.is_control() => {
          reader.bump();
          self.push_replacement();
        },
        _ => {
          let grapheme = reader.read_grapheme();
          self.push_grapheme(grapheme, self.base_style);
        },
      }
    }
  }

  fn finish(mut self) {
    self.flush();
  }

  fn flush(&mut self) {
    if self.run_start >= self.line.text.len() {
      return;
    }
    self.line.runs.push(Run {
      style: self.run_style,
      start: self.run_start,
    });
    self.run_start = self.line.text.len();
  }

  fn begin(&mut self, style: Style) {
    if style != self.run_style {
      self.flush();
      self.run_style = style;
    }
  }

  fn push_run(&mut self, text: &str) {
    self.begin(self.base_style);
    self.line.text.push_str(text);
    self.column += text.len();
  }

  fn push_grapheme(&mut self, grapheme: &str, style: Style) {
    self.begin(style);
    self.line.text.push_str(grapheme);
    self.column += UnicodeWidthStr::width(grapheme);
  }

  fn push_overstrike(&mut self, ch: char, attr: Attribute) {
    let mut style = self.base_style;
    style.attrs.set(attr);
    let mut buf = [0u8; 4];
    self.push_grapheme(ch.encode_utf8(&mut buf), style);
  }

  fn push_caret(&mut self, ch: char) {
    self.begin(self.base_style);
    self.line.text.push('^');
    self.line.text.push(caret(ch));
    self.column += 2;
  }

  fn push_replacement(&mut self) {
    self.begin(self.base_style);
    self.line.text.push('\u{fffd}');
    self.column += 1;
  }

  fn push_tab(&mut self, tab_width: NonZeroUsize) {
    let spaces = tab_width.get() - (self.column % tab_width);
    self.begin(self.base_style);
    self.line.text.extend(std::iter::repeat_n(' ', spaces));
    self.column += spaces;
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
    let mut state = LineBuilder::new(self);
    let mut rest = source;

    while let Some(backspace) = memchr(b'\x08', rest.as_bytes()) {
      let Some(prefix) = rest.get(..backspace) else {
        break;
      };
      let next = rest.get(backspace + 1..).and_then(|t| t.chars().next());
      let last = prefix.chars().next_back();

      if let Some((base, attr)) = overstrike(last, next) {
        let cut = prefix.len().saturating_sub(last.map_or(0, char::len_utf8));
        if let Some(clean_prefix) = prefix.get(..cut) {
          state.parse_text(clean_prefix, ansi_enabled, tab_width);
        }
        state.push_overstrike(base, attr);
        let next_start = backspace + 1 + base.len_utf8();
        rest = rest.get(next_start..).unwrap_or("");
      } else {
        state.parse_text(prefix, ansi_enabled, tab_width);
        state.push_caret('\x08');
        rest = rest.get(backspace + 1..).unwrap_or("");
      }
    }

    state.parse_text(rest, ansi_enabled, tab_width);
    state.finish();
  }

  /// Lazily lay out the display rows of this line as byte ranges into [`ParsedLine::text`].
  pub fn rows(&self, width: usize, wrap_mode: WrapMode) -> impl Iterator<Item = Range<usize>> + '_ {
    wrap::row_ranges(&self.text, width, wrap_mode)
  }

  /// Count display rows without retaining the row ranges.
  #[must_use]
  pub fn count_rows(&self, width: usize, wrap_mode: WrapMode) -> usize {
    self.rows(width, wrap_mode).count()
  }

  /// Printable text of this line with tabs expanded and control sequences stripped.
  #[must_use]
  pub fn text(&self) -> &str {
    &self.text
  }

  /// Width of this line in terminal columns.
  #[must_use]
  pub fn display_width(&self) -> usize {
    UnicodeWidthStr::width(self.text.as_str())
  }

  /// Byte range and leading padding covering the display columns `[left, left + width)`.
  ///
  /// A wide glyph that straddles the left edge is skipped rather than split; the skipped columns
  /// are reported in [`ColumnWindow::pad`].
  #[must_use]
  pub fn column_window(&self, left: usize, width: usize) -> ColumnWindow {
    if width == 0 || self.text.is_empty() {
      let end = self.text.len();
      return ColumnWindow {
        pad: 0,
        range: end..end,
      };
    }

    let limit = left.saturating_add(width);
    let mut column = 0usize;
    let mut start = None;
    let mut end = 0usize;

    for (index, grapheme) in self.text.grapheme_indices(true) {
      let glyph_width = UnicodeWidthStr::width(grapheme);
      let next = column + glyph_width;

      if glyph_width == 0 {
        if start.is_some() {
          end = index + grapheme.len();
        }
        column = next;
        continue;
      }

      if next <= left {
        column = next;
        continue;
      }
      if column < left {
        // A wide glyph straddles the left edge.
        column = next;
        continue;
      }
      if column >= limit || next > limit {
        break;
      }

      start.get_or_insert((index, column));
      end = index + grapheme.len();
      column = next;
    }

    match start {
      Some((start, start_column)) => ColumnWindow {
        pad: start_column - left,
        range: start..end,
      },
      None => ColumnWindow {
        pad: 0,
        range: self.text.len()..self.text.len(),
      },
    }
  }
}

// `git <subcommand> --help` likes to do things it's own way. `X\bX` is bold, `_\bX` is underlined.
// Man pages, I swear...
fn overstrike(last: Option<char>, next: Option<char>) -> Option<(char, Attribute)> {
  let first = last?;
  let next = next?;
  if first.is_control() || next.is_control() {
    return None;
  }

  let (base, attribute) = if first == next {
    (next, Attribute::Bold)
  } else if first == '_' {
    (next, Attribute::Underlined)
  } else if next == '_' {
    (first, Attribute::Underlined)
  } else {
    return None;
  };

  Some((base, attribute))
}

fn caret(ch: char) -> char {
  if ch == DEL {
    return '?';
  }
  u8::try_from(ch).map_or('\u{fffd}', |byte| char::from(byte + CARET_OFFSET))
}

#[cfg(test)]
mod tests;
