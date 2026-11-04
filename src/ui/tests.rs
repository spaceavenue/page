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
    StatusTemplate::default()
      .display(&context(false))
      .to_string(),
    "file.txt 123 lines 42% "
  );
  assert_eq!(
    StatusTemplate::default()
      .display(&context(true))
      .to_string(),
    "file.txt 123+ lines 42% [reading...]"
  );
}

#[test]
fn status_template_configurable() {
  let status = StatusTemplate {
    template: "{percent}% {filename} ({lines})".to_string(),
  };
  assert_eq!(
    status.display(&context(false)).to_string(),
    "42% file.txt (123)"
  );
}

#[test]
fn unexpanded_tokens_preserved() {
  let status = StatusTemplate {
    template: "{filename} {unknown} {unterminated".to_string(),
  };
  assert_eq!(
    status.display(&context(false)).to_string(),
    "file.txt {unknown} {unterminated"
  );
}

#[test]
fn token_like_filename_not_expanded() {
  let status = StatusTemplate {
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
