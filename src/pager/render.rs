use std::io;

use crossterm::cursor::MoveTo;
use crossterm::queue;
use crossterm::style::{Print, ResetColor, SetForegroundColor};
use crossterm::terminal::{Clear, ClearType};

use super::mode::Mode;
use super::{HELP, Pager};
use crate::config::options::Options;
use crate::pager::search::{SearchPrompt, SearchState};
use crate::text::WrapMode;
use crate::ui::viewport::Viewport;
use crate::ui::{BarContent, StatusContext};

impl Pager {
  /// Render the current frame to `out`.
  ///
  /// # Errors
  ///
  /// Returns an error if terminal output fails or a coordinate cannot be
  /// represented as a `u16`.
  pub fn render(&mut self, out: &mut impl io::Write) -> anyhow::Result<()> {
    // need to destructure here to keep borrow checker happy...
    let Self {
      opts,
      status_buf,
      viewport,
      search_state,
      ..
    } = self;
    match &self.mode {
      Mode::Normal => self.render_normal(out),
      Mode::Help => self.render_help(out),
      Mode::Search(prompt) => render_search(out, opts, viewport, status_buf, search_state, prompt),
    }
  }

  fn render_normal(&mut self, out: &mut impl io::Write) -> anyhow::Result<()> {
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
        },
        WrapMode::Soft | WrapMode::Hard => {
          let start_row = if skip == 0 {
            0
          } else {
            skip.min(self.parsed_buf.count_rows(content_width, self.opts.wrap) - 1)
          };

          let rows = self
            .parsed_buf
            .rows(content_width, self.opts.wrap)
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
        },
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
    self.opts.ui.render_bar(
      out,
      footer_row,
      self.viewport.width(),
      &BarContent::Message("Press any key to close."),
      &mut self.status_buf,
    )
  }

  fn render_status(&mut self, out: &mut impl io::Write) -> anyhow::Result<()> {
    let title = &self.title;
    let lines = self.buffer.line_count();
    let reading = self.buffer.is_streaming();
    let percent = self.viewport.percent(self.top.line, lines);

    let context = StatusContext::new(title, lines, percent, reading);
    let status_row = u16::try_from(self.viewport.last_row())?;
    self.opts.ui.render_bar(
      out,
      status_row,
      self.viewport.width(),
      &BarContent::Status(&context),
      &mut self.status_buf,
    )
  }
}

fn render_search(
  out: &mut impl io::Write,
  opts: &Options,
  viewport: &Viewport,
  status_buf: &mut String,
  search_state: &SearchState,
  prompt: &SearchPrompt,
) -> anyhow::Result<()> {
  status_buf.clear();
  let mode = search_state.mode.label();
  let query = prompt.query.as_str();
  let content = format!("{mode} / {query}|");
  let footer_row = u16::try_from(viewport.last_row())?;
  opts.ui.render_bar(
    out,
    footer_row,
    viewport.width(),
    &BarContent::Message(&content),
    status_buf,
  )
}
