pub mod status;
pub mod terminal;
pub mod viewport;

use std::fmt::{self};
use std::io::Write;

use crossterm::cursor::MoveTo;
use crossterm::queue;
use crossterm::style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor};
use crossterm::terminal::{Clear, ClearType};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub use self::status::{StatusContext, StatusTemplate};

/// Content displayed on the bottom line of the pager.
pub enum BarContent<'a> {
  Status(&'a StatusContext<'a>),
  Message(&'a str),
  // for stuff that isnt a static str
  Fmt(fmt::Arguments<'a>),
}

/// Clamps text to a display-column width, replaces controls, and pads spaces.
struct PaddedLine<'a> {
  text: &'a str,
  width: usize,
}

impl<'a> PaddedLine<'a> {
  const fn new(text: &'a str, width: usize) -> Self {
    Self { text, width }
  }
}

impl fmt::Display for PaddedLine<'_> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let mut used = 0usize;

    for grapheme in self.text.graphemes(true) {
      let control = grapheme.chars().any(char::is_control);
      let glyph_width = glyph_width(grapheme);
      if used + glyph_width > self.width {
        break;
      }
      if control {
        f.write_str(" ")?;
      } else {
        f.write_str(grapheme)?;
      }
      used += glyph_width;
    }

    for _ in used..self.width {
      f.write_str(" ")?;
    }
    Ok(())
  }
}

/// Pager colors and status-bar template.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ui {
  pub theme: Theme,
  pub status: StatusTemplate,
}

impl Ui {
  /// Render a formatted bottom bar to `out` at `row`, clamped to `width` columns.
  ///
  /// # Errors
  ///
  /// Returns an error if writing to `out` fails.
  pub fn render_bar(
    &self,
    out: &mut impl Write,
    row: u16,
    width: usize,
    content: &BarContent<'_>,
    buf: &mut String,
  ) -> anyhow::Result<()> {
    buf.clear();
    match content {
      BarContent::Status(ctx) => self.status.render(ctx, buf)?,
      BarContent::Message(msg) => buf.push_str(msg),
      BarContent::Fmt(args) => fmt::Write::write_fmt(&mut *buf, *args)?,
    }
    queue!(
      out,
      MoveTo(0, row),
      Clear(ClearType::CurrentLine),
      SetForegroundColor(self.theme.status_fg),
      SetBackgroundColor(self.theme.status_bg),
      Print(PaddedLine::new(buf, width)),
      ResetColor
    )?;
    Ok(())
  }
}

pub fn display_width(text: &str) -> usize {
  let mut width = 0usize;
  for grapheme in text.graphemes(true) {
    width += glyph_width(grapheme);
  }
  width
}

fn glyph_width(grapheme: &str) -> usize {
  let control = grapheme.chars().any(char::is_control);
  if control {
    1
  } else {
    UnicodeWidthStr::width(grapheme)
  }
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

#[cfg(test)]
mod tests;
