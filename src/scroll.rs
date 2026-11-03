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

#[cfg(test)]
mod tests {
  use super::*;

  /// Line heights used by tests: 0 -> 1, 1 -> 3, 2 -> 1, 3 -> 2.
  fn heights(line: usize) -> usize {
    match line {
      1 => 3,
      3 => 2,
      _ => 1,
    }
  }

  const fn at(line: usize, row: usize) -> Anchor {
    Anchor { line, row }
  }

  #[test]
  fn advance_within_line() {
    assert_eq!(advance(at(1, 0), 1, heights), at(1, 1));
    assert_eq!(advance(at(1, 1), 2, heights), at(2, 0));
  }

  #[test]
  fn advance_across_lines() {
    assert_eq!(advance(at(0, 0), 1, heights), at(1, 0));
    assert_eq!(advance(at(0, 0), 2, heights), at(1, 1));
    assert_eq!(advance(at(1, 2), 1, heights), at(2, 0));
  }

  #[test]
  fn advance_by_zero_noop() {
    assert_eq!(advance(at(2, 1), 0, heights), at(2, 1));
  }

  #[test]
  fn rewind_within_line() {
    assert_eq!(rewind(at(1, 2), 1, heights), at(1, 1));
    assert_eq!(rewind(at(1, 2), 2, heights), at(1, 0));
  }

  #[test]
  fn rewind_across_lines() {
    assert_eq!(rewind(at(1, 0), 1, heights), at(0, 0));
    assert_eq!(rewind(at(2, 0), 1, heights), at(1, 2));
    assert_eq!(rewind(at(3, 1), 2, heights), at(2, 0));
  }

  #[test]
  fn rewind_clamps_start() {
    assert_eq!(rewind(at(0, 0), 10, heights), at(0, 0));
    assert_eq!(rewind(at(1, 1), 10, heights), at(0, 0));
  }

  #[test]
  fn rewind_by_zero_noop() {
    assert_eq!(rewind(at(2, 1), 0, heights), at(2, 1));
  }

  #[test]
  fn advance_then_rewind_round_trips() {
    for start in [at(0, 0), at(1, 0), at(1, 2), at(3, 1)] {
      for rows in 0..10 {
        let moved = advance(start, rows, heights);
        assert_eq!(rewind(moved, rows, heights), start, "{start:?} + {rows}");
      }
    }
  }

  #[test]
  fn advance_out_of_bounds_row_does_not_panic() {
    // line 0 has height 1, but anchor has row 5. Advancing 1 row moves to line 1, row 1.
    assert_eq!(advance(at(0, 5), 1, heights), at(1, 1));
  }
}
