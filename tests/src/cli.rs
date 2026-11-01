use std::path::PathBuf;

use lexopt::Parser;
use page_rs::cli::{Cli, Input};
use page_rs::text::WrapMode;

fn parse_args(
  args: impl IntoIterator<Item = impl Into<std::ffi::OsString>>,
) -> anyhow::Result<Option<Cli>> {
  Cli::parse_args(Parser::from_args(args))
}

fn parse_args_expect_some(
  args: impl IntoIterator<Item = impl Into<std::ffi::OsString>>,
) -> Option<Cli> {
  Cli::parse_args(Parser::from_args(args)).expect("expected Option<Cli> got Error")
}

fn parse_args_expect_cli(args: impl IntoIterator<Item = impl Into<std::ffi::OsString>>) -> Cli {
  Cli::parse_args(Parser::from_args(args))
    .expect("expected Option<Cli> got Error")
    .expect("expected Cli")
}

#[test]
fn defaults() {
  let cli = parse_args_expect_cli(["file.txt"]);
  assert_eq!(cli.input, Input::Path(PathBuf::from("file.txt")));
  assert_eq!(cli.options.wrap, WrapMode::Soft);
  assert!(cli.options.ansi);
  assert_eq!(cli.options.tab_width.get(), 8);
  assert!(!cli.options.line_numbers);
  assert!(!cli.options.initial_follow);
}

#[test]
fn flags_parsed() {
  let cli = parse_args_expect_cli([
    "--follow",
    "--wrap",
    "hard",
    "--no-ansi",
    "--tab-width",
    "4",
    "--line-numbers",
    "-",
  ]);
  assert_eq!(cli.input, Input::Stdin);
  assert!(cli.options.initial_follow);
  assert_eq!(cli.options.wrap, WrapMode::Hard);
  assert!(!cli.options.ansi);
  assert_eq!(cli.options.tab_width.get(), 4);
  assert!(cli.options.line_numbers);
}

#[test]
fn wrap_option_parsed() {
  let cli = parse_args_expect_cli(["--wrap", "hard", "file"]);
  assert_eq!(cli.options.wrap, WrapMode::Hard);

  let cli = parse_args_expect_cli(["-w", "soft", "file"]);
  assert_eq!(cli.options.wrap, WrapMode::Soft);
}

#[test]
fn wrap_off_option_parsed() {
  let cli = parse_args_expect_cli(["-w", "off", "file"]);
  assert_eq!(cli.options.wrap, WrapMode::None);

  let cli = parse_args_expect_cli(["--no-wrap", "file"]);
  assert_eq!(cli.options.wrap, WrapMode::None);

  let cli = parse_args_expect_cli(["-W", "file"]);
  assert_eq!(cli.options.wrap, WrapMode::None);

  let cli = parse_args_expect_cli(["--wrap", "off", "file"]);
  assert_eq!(cli.options.wrap, WrapMode::None);
}

#[test]
fn tab_width_equals_form() {
  let cli = parse_args_expect_cli(["--tab-width=2", "file"]);
  assert_eq!(cli.options.tab_width.get(), 2);
}

#[test]
fn zero_tab_width_rejected() {
  assert!(parse_args(["--tab-width", "0", "file"]).is_err());
}

#[test]
fn oversized_tab_width_rejected() {
  assert!(parse_args(["--tab-width", "257", "file"]).is_err());
}

#[test]
fn help_returns_none() {
  assert!(parse_args_expect_some(["--help"]).is_none());
}
