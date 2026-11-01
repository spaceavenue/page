use crossterm::style::Color;

use super::*;

fn context(reading: bool) -> StatusContext<'static> {
  StatusContext {
    filename: "file.txt",
    lines: 123,
    percent: 42,
    reading,
  }
}

#[test]
fn default_status_template_matches() {
  assert_eq!(
    StatusBar::default().display(&context(false)).to_string(),
    "file.txt 123 lines 42% "
  );
  assert_eq!(
    StatusBar::default().display(&context(true)).to_string(),
    "file.txt 123+ lines 42% [reading...]"
  );
}

#[test]
fn status_template_configurable() {
  let status = StatusBar {
    template: "{percent}% {filename} ({lines})".to_string(),
  };
  assert_eq!(
    status.display(&context(false)).to_string(),
    "42% file.txt (123)"
  );
  assert_eq!(
    format!("{}", status.display(&context(false))),
    "42% file.txt (123)"
  );
}

#[test]
fn unknown_tokens_preserved() {
  let status = StatusBar {
    template: "{filename} {unknown}".to_string(),
  };
  assert_eq!(
    status.display(&context(false)).to_string(),
    "file.txt {unknown}"
  );
}

#[test]
fn token_like_filename_not_expanded() {
  let status = StatusBar {
    template: "{filename}".to_string(),
  };
  let context = StatusContext {
    filename: "{percent}",
    lines: 1,
    percent: 42,
    reading: false,
  };
  assert_eq!(status.display(&context).to_string(), "{percent}");
}

#[test]
fn unterminated_token_preserved() {
  let status = StatusBar {
    template: "file: {filename".to_string(),
  };
  assert_eq!(
    status.display(&context(false)).to_string(),
    "file: {filename"
  );
}

#[test]
fn default_theme_uses_hardcoded_colors() {
  let theme = Theme::default();
  assert_eq!(theme.status_fg, Color::White);
  assert_eq!(theme.status_bg, Color::Black);
  assert_eq!(theme.gutter_fg, Color::DarkGrey);
  assert_eq!(theme.filler_fg, Color::Blue);

  let ui = Ui::default();
  assert_eq!(ui.theme, theme);
  assert_eq!(ui.status, StatusBar::default());
}
