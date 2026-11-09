use std::num::NonZeroUsize;

use crate::search::settings::SearchSettings;
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
  /// Initial search mode (default = substring) and case sensitivity (default = Smart Case).
  pub search_settings: SearchSettings,
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
      SearchSettings::default(),
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
    search_settings: SearchSettings,
    ui: Ui,
  ) -> Self {
    Self {
      initial_follow,
      wrap,
      ansi,
      tab_width,
      line_numbers,
      search_settings,
      ui,
    }
  }
}
