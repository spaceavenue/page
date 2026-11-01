use std::ops::Range;

use unicode_width::UnicodeWidthChar;

/// Return display rows of `text`.
pub(super) fn row_ranges(
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
