use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::text::{WrapMode, none_bad};

struct RowBoundary {
  end: usize,
  next_start: usize,
}

impl RowBoundary {
  #[inline]
  const fn new(end: usize, next_start: usize) -> Self {
    Self { end, next_start }
  }
}

/// Return display rows of `text`.
pub(super) fn row_ranges(
  text: &str,
  width: usize,
  wrap_mode: WrapMode,
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

    let boundary = next_row(text, start, width, wrap_mode);
    if boundary.end <= start {
      return None;
    }

    let range = start..boundary.end;
    start = boundary.next_start;
    Some(range)
  })
}

/// Return the byte range of the next row and the byte offset where the row after it starts. These
/// differ when whitespace at the break is skipped.
fn next_row(text: &str, start: usize, width: usize, wrap_mode: WrapMode) -> RowBoundary {
  next_row_ascii(text, start, width, wrap_mode)
    .unwrap_or_else(|| next_row_grapheme(text, start, width, wrap_mode))
}

fn next_row_ascii(
  text: &str,
  start: usize,
  width: usize,
  wrap_mode: WrapMode,
) -> Option<RowBoundary> {
  let Some(rest) = text.get(start..) else {
    return Some(RowBoundary::new(text.len(), text.len()));
  };

  if rest.len() <= width {
    return Some(RowBoundary::new(text.len(), text.len()));
  }

  let window = rest.as_bytes().get(..width + 1).unwrap_or(rest.as_bytes());

  if !none_bad(window, |b| b.wrapping_sub(0x20) > 0x5e) {
    return None;
  }

  if *window.get(width)? == b' ' {
    return Some(RowBoundary::new(
      start + width,
      skip_wrap_spaces(text, start + width),
    ));
  }

  if matches!(wrap_mode, WrapMode::Soft)
    && let Some(p) = memchr::memrchr(b' ', window.get(..width)?)
  {
    let break_at = start + p + 1;
    if word_fits(text, break_at, width) {
      return Some(RowBoundary::new(break_at, break_at));
    }
    return Some(RowBoundary::new(start + width, start + width));
  }

  Some(RowBoundary::new(start + width, start + width))
}

fn next_row_grapheme(text: &str, start: usize, width: usize, wrap_mode: WrapMode) -> RowBoundary {
  let mut column = 0usize;
  let mut last_break = None;
  let mut end = start;

  let Some(rest) = text.get(start..) else {
    return RowBoundary::new(text.len(), text.len());
  };

  for (offset, grapheme) in rest.grapheme_indices(true) {
    let index = start + offset;
    let glyph_width = UnicodeWidthStr::width(grapheme);

    if glyph_width > 0 && column > 0 && column + glyph_width > width {
      // Breaking on whitespace itself is always safe: end the row before the overflow space and
      // skip the whole run at the start of the next.
      if is_wrap_space(grapheme) {
        return RowBoundary::new(index, skip_wrap_spaces(text, index));
      }

      if matches!(wrap_mode, WrapMode::Soft)
        && let Some(break_at) = last_break
      {
        // Moving a word that is wider than the whole row to a fresh row would just produce a row
        // containing only the indentation, so hard-break it where it is instead.
        return if word_fits(text, break_at, width) {
          RowBoundary::new(break_at, break_at)
        } else {
          RowBoundary::new(index, index)
        };
      }

      return RowBoundary::new(index, index);
    }

    column += glyph_width;
    end = index + grapheme.len();

    if is_wrap_space(grapheme) {
      last_break = Some(end);
    }
  }
  RowBoundary::new(end, end)
}

/// Whether the word starting at `start` fits in `width` columns.
fn word_fits(text: &str, start: usize, width: usize) -> bool {
  let mut column = 0usize;
  let Some(rest) = text.get(start..) else {
    return true;
  };

  for grapheme in rest.graphemes(true) {
    if is_wrap_space(grapheme) {
      return true;
    }
    column += UnicodeWidthStr::width(grapheme);
    if column > width {
      return false;
    }
  }

  true
}

fn is_wrap_space(grapheme: &str) -> bool {
  let mut chars = grapheme.chars();
  let Some(ch) = chars.next() else {
    return false;
  };
  chars.next().is_none() && is_wrap_space_char(ch)
}

const fn is_wrap_space_char(ch: char) -> bool {
  ch == ' ' || (ch.is_whitespace() && !matches!(ch, '\u{00a0}' | '\u{202f}'))
}

fn skip_wrap_spaces(text: &str, mut index: usize) -> usize {
  while let Some(rest) = text.get(index..) {
    let Some(grapheme) = rest.graphemes(true).next() else {
      break;
    };
    if !is_wrap_space(grapheme) {
      break;
    }
    index += grapheme.len();
  }
  index
}
