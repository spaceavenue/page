use std::io::Write;
use std::ops::Range;

use crossterm::queue;
use crossterm::style::{
  Attribute, Print, SetAttribute, SetAttributes, SetBackgroundColor, SetForegroundColor,
};
use unicode_width::UnicodeWidthChar;

use crate::ansi::{self, Style};
use crate::reader::Reader;

const ESC: char = '\x1b';
const DEL: char = '\x7f';
/// Caret notation shows control byte `b` as `^` followed by `b + 0x40`.
const CARET_OFFSET: u8 = 0x40;

/// A styled run in a parsed line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Run {
  /// Byte offset into [`ParsedLine::text`] where this run starts.
  start: usize,
  style: Style,
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
  pub fn parse(&mut self, raw: &[u8], ansi_enabled: bool, tab_width: usize) {
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
          let spaces = tab_width - (column % tab_width);
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

  /// Get the visual style active at byte offset `byte`.
  #[must_use]
  pub fn style_at(&self, byte: usize) -> Style {
    let first = self
      .runs
      .partition_point(|run| run.start <= byte)
      .saturating_sub(1);
    self
      .runs
      .get(first)
      .map_or_else(Style::default, |run| run.style)
  }

  /// Lazily lay out the display rows of this line as byte ranges into [`ParsedLine::text`].
  pub fn rows(&self, width: usize, word_wrap: bool) -> impl Iterator<Item = Range<usize>> + '_ {
    row_ranges(&self.text, width, word_wrap)
  }

  /// Count display rows without retaining the row ranges.
  #[must_use]
  pub fn count_rows(&self, width: usize, word_wrap: bool) -> usize {
    self.rows(width, word_wrap).count()
  }

  /// Render one display row, including the SGR commands needed to apply its
  /// style. The terminal style is reset at the end of the row.
  ///
  /// # Errors
  ///
  /// Returns an error if writing to `out` fails.
  pub fn render_range(&self, out: &mut impl Write, range: Range<usize>) -> anyhow::Result<()> {
    if range.is_empty() {
      return Ok(());
    }

    let mut current = Style::default();

    let first = self
      .runs
      .partition_point(|run| run.start <= range.start)
      .saturating_sub(1);
    let runs = self.runs.get(first..).unwrap_or_default();

    for (offset, run) in runs.iter().enumerate() {
      if run.start >= range.end {
        break;
      }

      let start = run.start.max(range.start);
      let next_start = runs
        .get(offset + 1)
        .map_or(self.text.len(), |next| next.start);
      let end = next_start.min(range.end);

      let Some(piece) = self.text.get(start..end).filter(|piece| !piece.is_empty()) else {
        continue;
      };

      if run.style != current {
        apply_style(out, run.style)?;
        current = run.style;
      }
      queue!(out, Print(piece))?;
    }

    if current != Style::default() {
      queue!(out, SetAttribute(Attribute::Reset))?;
    }

    Ok(())
  }
}

fn apply_style(out: &mut impl Write, style: Style) -> anyhow::Result<()> {
  queue!(out, SetAttribute(Attribute::Reset))?;

  if let Some(foreground) = style.fg {
    queue!(out, SetForegroundColor(foreground))?;
  }
  if let Some(background) = style.bg {
    queue!(out, SetBackgroundColor(background))?;
  }

  if !style.attrs.is_empty() {
    queue!(out, SetAttributes(style.attrs))?;
  }

  Ok(())
}

/// Return display rows of `text`.
fn row_ranges(
  text: &str,
  width: usize,
  word_wrap: bool,
) -> impl Iterator<Item = Range<usize>> + '_ {
  let width = width.max(1);
  let mut start = 0usize;
  let mut first = true;

  std::iter::from_fn(move || {
    if first {
      first = false;
      if text.is_empty() {
        return Some(0..0);
      }
    }

    let (end, next) = next_row(text, start, width, word_wrap);
    if end <= start {
      return None;
    }

    let range = start..end;
    start = next;
    Some(range)
  })
}

/// Return the byte range of the next row and the byte offset where the row after it starts. These
/// differ when whitespace at the break is skipped.
fn next_row(text: &str, start: usize, width: usize, word_wrap: bool) -> (usize, usize) {
  let mut column = 0usize;
  let mut last_break = None;
  let mut end = start;

  let Some(rest) = text.get(start..) else {
    return (text.len(), text.len());
  };

  for (offset, ch) in rest.char_indices() {
    let index = start + offset;
    let char_width = UnicodeWidthChar::width(ch).unwrap_or(0);

    if char_width > 0 && column > 0 && column + char_width > width {
      // Breaking on whitespace itself is always safe: end the row before the overflow space and
      // skip the whole run at the start of the next.
      if is_wrap_space(ch) {
        return (index, skip_wrap_spaces(text, index));
      }

      if word_wrap && let Some(break_at) = last_break {
        // Moving a word that is wider than the whole row to a fresh row would just produce a row
        // containing only the indentation, so hard-break it where it is instead.
        return if word_fits(text, break_at, width) {
          (break_at, break_at)
        } else {
          (index, index)
        };
      }

      return (index, index);
    }

    column += char_width;
    end = index + ch.len_utf8();

    if is_wrap_space(ch) {
      last_break = Some(end);
    }
  }

  (end, end)
}

/// Whether the word starting at `start` fits in `width` columns.
fn word_fits(text: &str, start: usize, width: usize) -> bool {
  let mut column = 0usize;
  let Some(rest) = text.get(start..) else {
    return true;
  };

  for ch in rest.chars() {
    if is_wrap_space(ch) {
      return true;
    }
    column += UnicodeWidthChar::width(ch).unwrap_or(0);
    if column > width {
      return false;
    }
  }

  true
}

fn caret(ch: char) -> char {
  if ch == DEL {
    return '?';
  }
  u8::try_from(ch).map_or('\u{fffd}', |byte| char::from(byte + CARET_OFFSET))
}

const fn is_wrap_space(ch: char) -> bool {
  ch == ' ' || (ch.is_whitespace() && !matches!(ch, '\u{00a0}' | '\u{202f}'))
}

fn skip_wrap_spaces(text: &str, mut index: usize) -> usize {
  while let Some(ch) = text.get(index..).and_then(|rest| rest.chars().next()) {
    if !is_wrap_space(ch) {
      break;
    }
    index += ch.len_utf8();
  }
  index
}
