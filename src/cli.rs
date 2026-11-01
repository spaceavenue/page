use std::io::IsTerminal;
use std::num::NonZeroUsize;
use std::path::PathBuf;

use anyhow::{Context, bail};
use lexopt::Arg::{Long, Short, Value};
use lexopt::{Parser, ValueExt};

use crate::options::Options;
use crate::text::WrapMode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
  Stdin,
  Path(PathBuf),
}

pub struct Cli {
  /// File or stdin.
  pub input: Input,
  /// Config options for the pager.
  pub options: Options,
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
    let mut o = Options::default();

    while let Some(arg) = parser.next()? {
      match arg {
        Short('f') | Long("follow") => o.initial_follow = true,
        Short('R') | Long("no-ansi") => o.ansi = false,
        Short('w') | Long("wrap") => o.wrap = get_wrap(&parser.value()?.string()?)?,
        Short('W') | Long("no-wrap") => o.wrap = WrapMode::None,
        Short('t') | Long("tab-width") => o.tab_width = get_tab_width(&parser.value()?.string()?)?,
        Short('N') | Long("line-numbers") => o.line_numbers = true,

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

    Ok(Some(Self { input, options: o }))
  }
}

fn get_wrap(val: &str) -> anyhow::Result<WrapMode> {
  match val {
    "hard" => Ok(WrapMode::Hard),
    "soft" => Ok(WrapMode::Soft),
    "off" => Ok(WrapMode::None),
    _ => bail!("invalid value for wrap"),
  }
}

fn get_tab_width(val: &str) -> anyhow::Result<NonZeroUsize> {
  let parsed = val
    .parse::<NonZeroUsize>()
    .context(format!("invalid tab width: {val}"))?;
  if parsed.get() > 256 {
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
    r"Usage: page-rs [OPTIONS] [- | FILE]
 Options:
    --follow, -f        follow stdin
    --wrap, -w MODE     soft (default), hard, or off
    --no-wrap, -W       alias for --wrap off
    --no-ansi, -R       do not interpret SGR escape sequences
    --tab-width N       columns between tab stops (default: 8, range: 1..=256)
    --line-numbers, -N  show line numbers
    --help, -h          print this message
"
  );
}
