use std::io::Write;

use crossterm::cursor::MoveTo;
use crossterm::queue;
use crossterm::style::{Print, ResetColor, SetBackgroundColor, SetForegroundColor};
use crossterm::terminal::{Clear, ClearType};

use super::mode::Mode;
use super::{HELP, Pager};
use crate::text::WrapMode;
use crate::ui::StatusContext;

// use crate::{StatusContext, WrapMode};

impl Pager {
  /// Render the current frame to `out`.
  ///
  /// # Errors
  ///
  /// Returns an error if terminal output fails or a coordinate cannot be
  /// represented as a `u16`.
  pub fn render(&mut self, out: &mut impl Write) -> anyhow::Result<()> {
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

      self.parsed.parse(raw, self.opts.ansi, self.opts.tab_width);

      match self.opts.wrap {
        WrapMode::None => {
          let range = self.parsed.column_range(self.left, content_width);
          let cursor_row = u16::try_from(screen_row)?;
          queue!(out, MoveTo(0, cursor_row), Clear(ClearType::CurrentLine))?;
          self.render_gutter(out, line, true, gutter)?;
          self.parsed.render_range(out, range)?;
          screen_row += 1;
        }
        WrapMode::Soft | WrapMode::Hard => {
          let word_wrap = self.opts.wrap == WrapMode::Soft;
          let start_row = if skip == 0 {
            0
          } else {
            skip.min(self.parsed.count_rows(content_width, word_wrap) - 1)
          };

          let rows = self
            .parsed
            .rows(content_width, word_wrap)
            .skip(start_row)
            .take(body - screen_row);

          for (offset, range) in rows.enumerate() {
            let cursor_row = u16::try_from(screen_row)?;
            queue!(out, MoveTo(0, cursor_row), Clear(ClearType::CurrentLine))?;
            let first_row = start_row == 0 && offset == 0;
            self.render_gutter(out, line, first_row, gutter)?;
            self.parsed.render_range(out, range)?;
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
    out: &mut impl Write,
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
        Print(format!("{:>number_width$} ", line + 1)),
        ResetColor
      )?;
    } else {
      queue!(out, Print(" ".repeat(gutter)))?;
    }

    Ok(())
  }

  fn render_help(&self, out: &mut impl Write) -> anyhow::Result<()> {
    queue!(out, MoveTo(0, 0), Clear(ClearType::All))?;

    for (row, line) in HELP.lines().take(self.viewport.body_height()).enumerate() {
      let cursor_row = u16::try_from(row)?;
      let text = line.chars().take(self.viewport.width()).collect::<String>();
      queue!(out, MoveTo(0, cursor_row), Print(text))?;
    }

    if self.viewport.height() > 0 {
      let footer_row = u16::try_from(self.viewport.last_row())?;
      let footer = "Press any key to close."
        .chars()
        .take(self.viewport.width())
        .collect::<String>();
      queue!(out, MoveTo(0, footer_row), Print(footer))?;
    }

    Ok(())
  }

  fn render_status(&mut self, out: &mut impl Write) -> anyhow::Result<()> {
    use std::fmt::Write as _;

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
      Print(format_args!(
        "{status:<width$}",
        status = self.status_buf,
        width = self.viewport.width()
      )),
      ResetColor
    )?;
    Ok(())
  }
}
