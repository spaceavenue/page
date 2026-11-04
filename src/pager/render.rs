use std::fmt::{self, Write};
use std::io;

use crossterm::cursor::MoveTo;
use crossterm::queue;
use crossterm::style::{Print, ResetColor, SetBackgroundColor, SetForegroundColor};
use crossterm::terminal::{Clear, ClearType};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::mode::Mode;
use super::{HELP, Pager};
use crate::text::WrapMode;
use crate::ui::{StatusContext, StatusDisplay};

/// Status line clamped to a display-column width.
struct StatusLine<'a> {
  text: &'a str,
  width: usize,
}
impl<'a> StatusLine<'a> {
  fn new(text: &'a str, width: usize) -> Self {
    Self { text, width }
  }
}

impl fmt::Display for StatusLine<'_> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let mut used = 0usize;

    for grapheme in self.text.graphemes(true) {
      let control = grapheme.chars().any(char::is_control);
      let glyph_width = if control {
        1
      } else {
        UnicodeWidthStr::width(grapheme)
      };
      if used + glyph_width > self.width {
        break;
      }
      control
        .then(|| f.write_str(" "))
        .unwrap_or_else(|| f.write_str(grapheme))?;
      used += glyph_width;
    }

    for _ in used..self.width {
      f.write_str(" ")?;
    }
    Ok(())
  }
}

impl Pager {
  /// Render the current frame to `out`.
  ///
  /// # Errors
  ///
  /// Returns an error if terminal output fails or a coordinate cannot be
  /// represented as a `u16`.
  pub fn render(&mut self, out: &mut impl io::Write) -> anyhow::Result<()> {
    if matches!(&self.mode, Mode::Help) {
      self.render_help(out)?;
      out.flush()?;
      return Ok(());
    }
    self
      .buffer
      .index_till_line(self.viewport.bottom_line(self.top.line));

    let body = self.viewport.body_height();
    let content_width = self.content_width();
    let gutter = self.gutter_width();
    let mut line = self.top.line;
    let mut skip = self.top.row;
    let mut screen_row = 0usize;

    while screen_row < body {
      let Some(raw) = self.buffer.line_bytes(line) else {
        break;
      };

      self
        .parsed_buf
        .parse(raw, self.opts.ansi, self.opts.tab_width);

      match self.opts.wrap {
        WrapMode::None => {
          let window = self.parsed_buf.column_window(self.left, content_width);
          let cursor_row = u16::try_from(screen_row)?;
          queue!(out, MoveTo(0, cursor_row), Clear(ClearType::CurrentLine))?;
          self.render_gutter(out, line, true, gutter)?;
          if window.pad > 0 {
            queue!(
              out,
              Print(format_args!("{:width$}", "", width = window.pad))
            )?;
          }
          self.parsed_buf.render_range(out, window.range)?;
          screen_row += 1;
        }
        WrapMode::Soft | WrapMode::Hard => {
          let word_wrap = self.opts.wrap == WrapMode::Soft;
          let start_row = if skip == 0 {
            0
          } else {
            skip.min(self.parsed_buf.count_rows(content_width, word_wrap) - 1)
          };

          let rows = self
            .parsed_buf
            .rows(content_width, word_wrap)
            .skip(start_row)
            .take(body - screen_row);

          for (offset, range) in rows.enumerate() {
            let cursor_row = u16::try_from(screen_row)?;
            queue!(out, MoveTo(0, cursor_row), Clear(ClearType::CurrentLine))?;
            let first_row = start_row == 0 && offset == 0;
            self.render_gutter(out, line, first_row, gutter)?;
            self.parsed_buf.render_range(out, range)?;
            screen_row += 1;
          }
        }
      }

      line += 1;
      skip = 0;
    }

    for row in screen_row..body {
      let cursor_row = u16::try_from(row)?;
      queue!(
        out,
        MoveTo(0, cursor_row),
        Clear(ClearType::CurrentLine),
        SetForegroundColor(self.opts.ui.theme.filler_fg),
        Print("~"),
        ResetColor
      )?;
    }

    self.render_status(out)?;
    out.flush()?;
    Ok(())
  }
  fn render_gutter(
    &self,
    out: &mut impl io::Write,
    line: usize,
    first_row: bool,
    gutter: usize,
  ) -> anyhow::Result<()> {
    if gutter == 0 {
      return Ok(());
    }

    if first_row {
      let number_width = gutter - 1;
      queue!(
        out,
        SetForegroundColor(self.opts.ui.theme.gutter_fg),
        Print(format_args!("{:>number_width$} ", line + 1)),
        ResetColor
      )?;
    } else {
      queue!(out, Print(format_args!("{:width$}", "", width = gutter)))?;
    }

    Ok(())
  }

  fn render_help(&mut self, out: &mut impl io::Write) -> anyhow::Result<()> {
    queue!(out, MoveTo(0, 0), Clear(ClearType::All))?;

    for (row, line) in HELP.lines().take(self.viewport.body_height()).enumerate() {
      let cursor_row = u16::try_from(row)?;
      let text = line.chars().take(self.viewport.width()).collect::<String>();
      queue!(out, MoveTo(0, cursor_row), Print(text))?;
    }

    let footer_row = u16::try_from(self.viewport.last_row())?;
    self.status_buf.clear();
    write!(&mut self.status_buf, "{}", "Press any key to close.")?;
    queue!(
      out,
      MoveTo(0, footer_row),
      Print(StatusLine::new(&self.status_buf, self.viewport.width()))
    )?;

    Ok(())
  }

  fn render_status(&mut self, out: &mut impl io::Write) -> anyhow::Result<()> {
    let filename = self.buffer.filename();
    let lines = self.buffer.line_count();
    let reading = self.buffer.is_streaming();
    let percent = self.viewport.percent(self.top.line, lines);

    let context = StatusContext::new(filename, lines, percent, reading);
    self.status_buf.clear();
    write!(
      &mut self.status_buf,
      "{}",
      self.opts.ui.status.display(&context)
    )?;

    let status_row = u16::try_from(self.viewport.last_row())?;

    queue!(
      out,
      MoveTo(0, status_row),
      Clear(ClearType::CurrentLine),
      SetForegroundColor(self.opts.ui.theme.status_fg),
      SetBackgroundColor(self.opts.ui.theme.status_bg),
      Print(StatusLine::new(&self.status_buf, self.viewport.width())),
      ResetColor
    )?;
    Ok(())
  }
}
