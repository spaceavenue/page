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
