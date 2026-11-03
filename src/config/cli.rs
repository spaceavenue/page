use std::io::IsTerminal;
use std::num::NonZeroUsize;
use std::path::PathBuf;

use anyhow::bail;
use clap::{Parser, ValueEnum, value_parser};

use crate::config::options::Options;
use crate::text::WrapMode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
  Stdin,
  Path(PathBuf),
}

/// CLI choices for `--wrap` option.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
enum CliWrap {
  #[default]
  Soft,
  Hard,
  #[value(name = "off", alias = "none")]
  None,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Parser)]
#[command(version, about, author)]
struct Args {
  /// Follow stdin as new data arrives.
  #[arg(short = 'f', long = "follow")]
  follow: bool,

  /// Set line wrapping mode. Conflicts with `--no-wrap`.
  #[arg(short = 'w', long = "wrap", value_enum, conflicts_with = "no_wrap")]
  wrap: Option<CliWrap>,

  /// Alias for --wrap off. Conflicts with `--wrap`.
  #[arg(short = 'W', long = "no-wrap")]
  no_wrap: bool,

  /// Set tab stop width in columns (1..=256).
  #[arg(short = 't', long = "tab-width", default_value = "8", value_parser = value_parser!(u16).range(1..=256))]
  tab_width: u16,

  /// Turn ANSI SGR escape sequence parsing off.
  #[arg(short = 'R', long = "no-ansi")]
  no_ansi: bool,

  /// Show line numbers in gutter.
  #[arg(short = 'N', long = "line-numbers", default_value = "false")]
  line_numbers: bool,

  /// File to display, or '-' for stdin.
  #[arg(value_name = "FILE")]
  file: Option<PathBuf>,
}

pub struct Cli {
  /// File or stdin.
  pub input: Input,
  /// Config options for the pager.
  pub options: Options,
}

impl TryFrom<Args> for Cli {
  type Error = anyhow::Error;

  fn try_from(args: Args) -> Result<Self, Self::Error> {
    let mut options = Options::default();

    let input = match args.file {
      Some(p) if p.as_os_str() == "-" => Input::Stdin,
      Some(p) => Input::Path(p),
      None if std::io::stdin().is_terminal() => bail!("missing filename or piped input."),
      None => Input::Stdin,
    };

    if args.no_wrap {
      options.wrap = WrapMode::None;
    } else if let Some(wrap) = args.wrap {
      options.wrap = match wrap {
        CliWrap::Soft => WrapMode::Soft,
        CliWrap::Hard => WrapMode::Hard,
        CliWrap::None => WrapMode::None,
      }
    }

    options.tab_width = NonZeroUsize::new(usize::from(args.tab_width)).unwrap_or(NonZeroUsize::MIN);

    options.initial_follow = args.follow;
    options.ansi = !args.no_ansi;
    options.line_numbers = args.line_numbers;

    Ok(Self { input, options })
  }
}

impl Cli {
  /// Parse command-line arguments from `std::env::args_os`.
  ///
  /// # Errors
  ///
  /// Returns an error if any arguments fail to parse or are invalid.
  pub fn parse() -> anyhow::Result<Self> {
    Self::try_from(Args::parse())
  }
}

#[cfg(test)]
mod tests {
  use std::iter;

  use super::*;

  fn parse_args<I, T>(args: I) -> Cli
  where
    I: IntoIterator<Item = T>,
    T: Into<String> + Clone,
  {
    try_parse_args(args).expect("expected Cli")
  }

  fn try_parse_args<I, T>(args: I) -> anyhow::Result<Cli>
  where
    I: IntoIterator<Item = T>,
    T: Into<String> + Clone,
  {
    let args = iter::once(String::from("page")).chain(args.into_iter().map(Into::into));
    let args = Args::try_parse_from(args)?;
    Cli::try_from(args)
  }

  #[test]
  fn defaults() {
    let cli = parse_args(vec!["file.txt"]);
    assert_eq!(cli.input, Input::Path(PathBuf::from("file.txt")));
    assert_eq!(cli.options.wrap, WrapMode::Soft);
    assert!(cli.options.ansi);
    assert_eq!(cli.options.tab_width.get(), 8);
    assert!(!cli.options.line_numbers);
    assert!(!cli.options.initial_follow);
  }

  #[test]
  fn flags_parsed() {
    let cli = parse_args([
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
    let cli = parse_args(["--wrap", "hard", "file"]);
    assert_eq!(cli.options.wrap, WrapMode::Hard);

    let cli = parse_args(["-w", "soft", "file"]);
    assert_eq!(cli.options.wrap, WrapMode::Soft);
  }

  #[test]
  fn wrap_off_option_parsed() {
    let cli = parse_args(["-w", "off", "file"]);
    assert_eq!(cli.options.wrap, WrapMode::None);

    let cli = parse_args(["-W", "file"]);
    assert_eq!(cli.options.wrap, WrapMode::None);

    let cli = parse_args(["--wrap", "off", "file"]);
    assert_eq!(cli.options.wrap, WrapMode::None);

    let cli = parse_args(["--no-wrap", "file"]);
    assert_eq!(cli.options.wrap, WrapMode::None);
  }

  #[test]
  fn no_wrap_conficts_wrap() {
    assert!(try_parse_args(["--wrap", "soft", "--no-wrap"]).is_err());
    assert!(try_parse_args(["-w", "off", "-W"]).is_err());
  }

  #[test]
  fn tab_width_equals_form() {
    let cli = parse_args(["--tab-width=2", "file"]);
    assert_eq!(cli.options.tab_width.get(), 2);
  }

  #[test]
  fn zero_tab_width_rejected() {
    assert!(try_parse_args(["--tab-width", "0", "file"]).is_err());
  }

  #[test]
  fn oversized_tab_width_rejected() {
    assert!(try_parse_args(["--tab-width", "257", "file"]).is_err());
  }
}
