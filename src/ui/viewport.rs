/// Terminal dimensions and the layout values derived from them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Viewport {
  width: usize,
  height: usize,
}

/// Below this width a line-number gutter leaves too little room for content.
const MIN_WIDTH_WITH_GUTTER: usize = 8;
/// Keep line numbers aligned for small files without a fixed wide gutter.
const MIN_GUTTER_DIGITS: usize = 3;

impl Viewport {
  /// Create a viewport from terminal dimensions.
  #[must_use]
  pub const fn new(width: usize, height: usize) -> Self {
    Self { width, height }
  }

  /// Get viewport width.
  #[must_use]
  pub const fn width(self) -> usize {
    self.width
  }

  /// Get viewport height.
  #[must_use]
  pub const fn height(self) -> usize {
    self.height
  }

  /// Index of the last terminal row, used by the status bar and help footer.
  #[must_use]
  pub const fn last_row(self) -> usize {
    self.height.saturating_sub(1)
  }

  /// Number of rows available for content.
  #[must_use]
  pub const fn body_height(self) -> usize {
    self.last_row()
  }

  /// Width available for content after reserving `gutter` columns.
  #[must_use]
  pub fn content_width(self, gutter: usize) -> usize {
    self.width.saturating_sub(gutter).max(1)
  }

  /// Line index where the visible line range ends.
  #[must_use]
  pub const fn bottom_line(self, top: usize) -> usize {
    top.saturating_add(self.body_height())
  }

  /// Scroll percentage for the top line and total line count.
  #[must_use]
  pub fn percent(self, top_line: usize, total_lines: usize) -> usize {
    if total_lines == 0 {
      return 100;
    }

    let max_top = total_lines.saturating_sub(self.body_height());
    if max_top == 0 {
      return 100;
    }

    let top = u64::try_from(top_line.min(max_top)).unwrap_or(0);
    let max_top = u64::try_from(max_top).unwrap_or(1).max(1);
    let percent = top.saturating_mul(100) / max_top;
    usize::try_from(percent).unwrap_or(100).min(100)
  }

  /// Width of the line-number gutter (zero when line numbers disabled).
  #[must_use]
  pub fn gutter_width(self, line_numbers: bool, line_count: usize) -> usize {
    if !line_numbers || self.width < MIN_WIDTH_WITH_GUTTER {
      return 0;
    }

    (((line_count.max(1).checked_ilog10().unwrap_or(0) + 1) as usize).max(MIN_GUTTER_DIGITS) + 1)
      .min(self.width - 1)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn dimensions_and_body_height() {
    let viewport = Viewport::new(80, 24);
    assert_eq!(viewport.width(), 80);
    assert_eq!(viewport.height(), 24);
    assert_eq!(viewport.body_height(), 23);
    assert_eq!(viewport.last_row(), 23);
  }

  #[test]
  fn zero_height_has_no_body_rows() {
    let viewport = Viewport::new(80, 0);
    assert_eq!(viewport.body_height(), 0);
    assert_eq!(viewport.last_row(), 0);
  }

  #[test]
  fn content_width_reserves_gutter_and_never_reaches_zero() {
    let viewport = Viewport::new(80, 24);
    assert_eq!(viewport.content_width(0), 80);
    assert_eq!(viewport.content_width(4), 76);
    assert_eq!(viewport.content_width(100), 1);
  }

  #[test]
  fn bottom_line_saturates() {
    let viewport = Viewport::new(80, 24);
    assert_eq!(viewport.bottom_line(0), 23);
    assert_eq!(viewport.bottom_line(100), 123);
    assert_eq!(viewport.bottom_line(usize::MAX), usize::MAX);
  }

  #[test]
  fn gutter_width_grows_with_line_count() {
    let viewport = Viewport::new(80, 24);
    assert_eq!(viewport.gutter_width(false, 1_000), 0);
    assert_eq!(viewport.gutter_width(true, 1), 4);
    assert_eq!(viewport.gutter_width(true, 120), 4);
    assert_eq!(viewport.gutter_width(true, 1_000), 5);
  }

  #[test]
  fn gutter_disabled_on_narrow_terminals() {
    let viewport = Viewport::new(7, 24);
    assert_eq!(viewport.gutter_width(true, 1_000), 0);
  }

  #[test]
  fn percent_zero_at_top_and_hundred_at_bottom() {
    let viewport = Viewport::new(80, 24);
    assert_eq!(viewport.percent(0, 120), 0);
    assert_eq!(viewport.percent(97, 120), 100);
    assert_eq!(viewport.percent(200, 120), 100);
  }

  #[test]
  fn short_files_always_one_hundred_percent() {
    let viewport = Viewport::new(80, 24);
    assert_eq!(viewport.percent(0, 10), 100);
    assert_eq!(viewport.percent(0, 0), 100);
  }
}
