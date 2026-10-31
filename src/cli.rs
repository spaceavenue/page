use std::io::IsTerminal;
use std::path::PathBuf;

use anyhow::{Context, bail};
use lexopt::Arg::{Long, Short, Value};
use lexopt::{Parser, ValueExt};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
  Stdin,
  Path(PathBuf),
}

pub struct Cli {
  /// File or stdin.
  pub input: Input,
  /// Follow mode (default = false).
  pub follow: bool,
  /// When wrapping, slice words across lines (default = false).
  pub hard_wrap: bool,
  /// Control SGR parsing (default = true).
  pub ansi: bool,
  /// Width of tab characters (default = 8).
  pub tab_width: usize,
}

impl Cli {
  /// Parse command-line arguments from `std::env::args_os`.
  ///
  /// # Errors
  ///
  /// Returns an error if any arguments fail to parse or are invalid.
  pub fn parse() -> anyhow::Result<Option<Self>> {
    Self::parse_args(Parser::from_env())
  }

  /// Parse command-line options using the given [`Parser`].
  ///
  /// # Errors
  ///
  /// Returns an error if any arguments fail to parse or are invalid.
  pub fn parse_args(mut parser: Parser) -> anyhow::Result<Option<Self>> {
    let mut input = None;
    let mut follow = false;
    let mut hard_wrap = false;
    let mut ansi = true;
    let mut tab_width = 8usize;

    while let Some(arg) = parser.next()? {
      match arg {
        Short('f') | Long("follow") => follow = true,
        Short('R') | Long("no-ansi") => ansi = false,
        Short('w') | Long("wrap") => hard_wrap = get_wrap(&parser.value()?.string()?)?,
        Short('t') | Long("tab-width") => tab_width = get_tab_width(&parser.value()?.string()?)?,

        Value(val) if val == "-" => input = Some(Input::Stdin),
        Value(_) if input.is_some() => bail!("unexpected extra argument"),
        Value(val) => input = Some(Input::Path(PathBuf::from(val))),

        Short('h') | Long("help") => {
          get_help();
          return Ok(None);
        }
        _ => return Err(arg.unexpected().into()),
      }
    }

    let input = get_input(input)?;

    Ok(Some(Self {
      input,
      follow,
      hard_wrap,
      ansi,
      tab_width,
    }))
  }
}

fn get_wrap(val: &str) -> anyhow::Result<bool> {
  match val {
    "hard" => Ok(true),
    "soft" => Ok(false),
    _ => bail!("invalid value for wrap"),
  }
}

fn get_tab_width(val: &str) -> anyhow::Result<usize> {
  let parsed = val
    .parse::<usize>()
    .context(format!("invalid tab width: {val}"))?;
  if !(1..=256).contains(&parsed) {
    bail!("tab width must be between 1 and 256")
  }
  Ok(parsed)
}

fn get_input(input: Option<Input>) -> anyhow::Result<Input> {
  match input {
    Some(input) => Ok(input),
    None if std::io::stdin().is_terminal() => bail!("missing filename or piped input"),
    None => Ok(Input::Stdin),
  }
}

fn get_help() {
  println!(
    "Usage: page-rs [OPTIONS] [- | FILE]\n\n\
     Options:\n\
     \x20\x20--follow, -f     follow mode\n\
     \x20\x20--wrap, -w       wrapping mode: soft (default) or hard\n\
     \x20\x20--no-ansi, -R    do not interpret SGR escape sequences\n\
     \x20\x20--tab-width N    columns between tab stops (default: 8)\n\
     \x20\x20--help, -h       print this message"
  );
}
