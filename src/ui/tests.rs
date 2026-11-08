#![allow(clippy::literal_string_with_formatting_args)] // status templates are not Rust format strings

use super::*;

fn context(reading: bool) -> StatusContext<'static> {
  StatusContext {
    filename: "file.txt",
    lines: 123,
    percent: 42,
    reading,
    notice: "",
  }
}

fn render(status: &StatusTemplate, context: &StatusContext<'_>) -> String {
  let mut out = String::new();
  assert!(status.render(context, &mut out).is_ok());
  out
}

fn compiled(source: &str) -> StatusTemplate {
  let result = StatusTemplate::compile(source);
  assert!(result.is_ok(), "template should compile: {source}");
  let Ok(status) = result else {
    return StatusTemplate::default();
  };
  status
}

#[test]
fn default_status_template_matches() {
  let status = StatusTemplate::default();
  assert_eq!(render(&status, &context(false)), "file.txt 123 lines 42%  ");
  assert_eq!(
    render(&status, &context(true)),
    "file.txt 123+ lines 42% [reading...] "
  );

  let context = StatusContext {
    notice: "[search]: wrapped around document",
    ..context(false)
  };
  assert_eq!(
    render(&status, &context),
    "file.txt 123 lines 42%  [search]: wrapped around document"
  );
}

#[test]
fn status_template_configurable() {
  let source = "{percent}% {filename} ({lines})";
  let status = compiled(source);
  assert_eq!(render(&status, &context(false)), "42% file.txt (123)");
}

#[test]
fn unknown_and_unclosed_tokens_are_rejected() {
  for source in ["{filename} {unknown}", "{filename", "{}", "a\nb"] {
    assert!(StatusTemplate::compile(source).is_err());
  }
}

#[test]
fn escaped_braces_are_literal() {
  let source = "{{filename}} {filename}";
  let status = compiled(source);
  assert_eq!(render(&status, &context(false)), "{filename} file.txt");
}

#[test]
fn alignment_pads_values() {
  let source = "{percent:>4}|{percent:<4}|{percent:^4}|{percent:4}";
  let status = compiled(source);
  assert_eq!(render(&status, &context(false)), "  42|42  | 42 |  42");
}

#[test]
fn text_defaults_left_and_counts_default_right() {
  let source = "{filename:10}|{percent:4}";
  let status = compiled(source);
  assert_eq!(render(&status, &context(false)), "file.txt  |  42");
}

#[test]
fn invalid_format_specs_are_rejected() {
  for source in ["{percent:>0}", "{percent:x}", "{percent:>>4}"] {
    assert!(StatusTemplate::compile(source).is_err());
  }
}

#[test]
fn token_like_filename_not_expanded() {
  let source = "{filename}";
  let filename = "{percent}";
  let status = compiled(source);
  let context = StatusContext {
    filename,
    ..StatusContext::default()
  };
  assert_eq!(render(&status, &context), "{percent}");
}
