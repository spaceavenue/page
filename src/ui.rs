pub mod terminal;
pub mod viewport;

use std::fmt::{self, Display, Formatter, Write as _};

use crossterm::style::Color;

use crate::text::reader::Reader;

const TOKEN_FILENAME: &str = "filename";
const TOKEN_LINES: &str = "lines";
const TOKEN_PLUS: &str = "plus";
const TOKEN_PERCENT: &str = "percent";
const TOKEN_STATE: &str = "state";

/// Pager colors and status-bar template.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ui {
  pub theme: Theme,
  pub status: StatusBar,
}

/// Colors used by the pager.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Theme {
  pub status_fg: Color,
  pub status_bg: Color,
  pub gutter_fg: Color,
  pub filler_fg: Color,
}

impl Default for Theme {
  fn default() -> Self {
    Self {
      status_fg: Color::White,
      status_bg: Color::Black,
      gutter_fg: Color::DarkGrey,
      filler_fg: Color::Blue,
    }
  }
}

/// Values available to the status-bar template.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StatusContext<'a> {
  pub filename: &'a str,
  pub lines: usize,
  pub percent: usize,
  pub reading: bool,
}

impl<'a> StatusContext<'a> {
  #[must_use]
  pub const fn new(filename: &'a str, lines: usize, percent: usize, reading: bool) -> Self {
    Self {
      filename,
      lines,
      percent,
      reading,
    }
  }
}

/// Template for the status bar. More tokens will be added (probably) with a config file parser.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusBar {
  pub template: String,
}

impl Default for StatusBar {
  fn default() -> Self {
    Self {
      template: "{filename} {lines}{plus} lines {percent}% {state}".to_string(),
    }
  }
}

/// Adapter for formatting a status bar.
#[derive(Clone, Copy, Debug)]
pub struct StatusDisplay<'a> {
  status: &'a StatusBar,
  context: &'a StatusContext<'a>,
}

impl Display for StatusDisplay<'_> {
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    let mut reader = Reader::new(&self.status.template);
    while let Some(ch) = reader.bump() {
      if ch != '{' {
        f.write_char(ch)?;
        continue;
      }

      let token = reader.read_until('}');
      if !reader.eat('}') {
        f.write_char('{')?;
        f.write_str(token)?;
        break;
      }

      match token {
        TOKEN_FILENAME => f.write_str(self.context.filename)?,
        TOKEN_LINES => write!(f, "{}", self.context.lines)?,
        TOKEN_PLUS if self.context.reading => f.write_char('+')?,
        TOKEN_PERCENT => write!(f, "{}", self.context.percent)?,
        TOKEN_STATE if self.context.reading => f.write_str("[reading...]")?,
        TOKEN_PLUS | TOKEN_STATE => {}
        _ => {
          f.write_char('{')?;
          f.write_str(token)?;
          f.write_char('}')?;
        }
      }
    }

    Ok(())
  }
}

impl StatusBar {
  /// Return a [`Display`] adapter for the given status context.
  #[must_use]
  pub const fn display<'a>(&'a self, context: &'a StatusContext<'a>) -> StatusDisplay<'a> {
    StatusDisplay {
      status: self,
      context,
    }
  }
}

#[cfg(test)]
mod tests;
