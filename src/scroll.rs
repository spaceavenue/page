/// A viewport position: the logical line and the display row within it.
///
/// `row` is zero for the first display row of the line. When wrapping is disabled every line has
/// exactly one row, so `row` is always zero.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Anchor {
  pub line: usize,
  pub row: usize,
}

/// Move `anchor` forward by `rows` display rows.
#[must_use]
pub fn advance(
  mut anchor: Anchor,
  mut rows: usize,
  mut line_rows: impl FnMut(usize) -> usize,
) -> Anchor {
  while rows > 0 {
    let height = line_rows(anchor.line).max(1);
    let remaining = height.saturating_sub(anchor.row);

    if rows < remaining {
      anchor.row += rows;
      break;
    }

    rows -= remaining;
    anchor.line += 1;
    anchor.row = 0;
  }

  anchor
}

/// Move `anchor` backward by `rows` display rows.
#[must_use]
pub fn rewind(
  mut anchor: Anchor,
  mut rows: usize,
  mut line_rows: impl FnMut(usize) -> usize,
) -> Anchor {
  while rows > 0 {
    if anchor.row >= rows {
      anchor.row -= rows;
      break;
    }

    rows -= anchor.row;

    if anchor.line == 0 {
      return Anchor::default();
    }

    anchor.line -= 1;
    anchor.row = line_rows(anchor.line).max(1);
  }

  anchor
}
