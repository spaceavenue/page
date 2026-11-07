use std::num::NonZeroUsize;

use crate::search::{CaseSensitivity, SearchMode};
use crate::text::WrapMode;
use crate::ui::Ui;

/// Config options for the pager.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
  /// Start in follow mode (default = false).
  pub initial_follow: bool,
  /// How long lines are displayed (default = soft wrap).
  pub wrap: WrapMode,
  /// Control SGR parsing (default = true).
  pub ansi: bool,
  /// Width of tab characters (default = 8).
  pub tab_width: NonZeroUsize,
  /// Show a line-number gutter (default = false).
  pub line_numbers: bool,
  /// Initial search mode (default = substring).
  pub search_mode: SearchMode,
  /// Initial case case sensitivity of searches (default = Smart Case)
  pub case_sensitivity: CaseSensitivity,
  /// Pager colors and status bar config.
  pub ui: Ui,
}

impl Default for Options {
  fn default() -> Self {
    Self::new(
      false,
      WrapMode::default(),
      true,
      NonZeroUsize::new(8).unwrap_or(NonZeroUsize::MIN),
      false,
      SearchMode::default(),
      CaseSensitivity::default(),
      Ui::default(),
    )
  }
}

impl Options {
  /// Create pager options with the given settings.
  #[must_use]
  #[allow(clippy::too_many_arguments)]
  pub const fn new(
    initial_follow: bool,
    wrap: WrapMode,
    ansi: bool,
    tab_width: NonZeroUsize,
    line_numbers: bool,
    search_mode: SearchMode,
    case_sensitivity: CaseSensitivity,
    ui: Ui,
  ) -> Self {
    Self {
      initial_follow,
      wrap,
      ansi,
      tab_width,
      line_numbers,
      search_mode,
      case_sensitivity,
      ui,
    }
  }
}
