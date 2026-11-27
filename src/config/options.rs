use std::num::NonZeroUsize;

use crate::search::settings::SearchSettings;
use crate::text::WrapMode;
use crate::ui::Ui;

/// Config options for the pager.
#[allow(clippy::struct_excessive_bools)]
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
  /// Print directly to screen.
  pub print_if_fits: bool,
}

impl Default for Options {
  fn default() -> Self {
    Self {
      initial_follow: false,
      wrap: WrapMode::default(),
      ansi: true,
      tab_width: NonZeroUsize::new(8).unwrap_or(NonZeroUsize::MIN),
      line_numbers: false,
      search_settings: SearchSettings::default(),
      ui: Ui::default(),
      print_if_fits: false,
    }
  }
}
