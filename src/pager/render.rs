use std::io::{self};

use crossterm::cursor::{MoveTo, SetCursorStyle};
use crossterm::queue;
use crossterm::style::{Attributes, Print, ResetColor, SetForegroundColor};
use crossterm::terminal::{Clear, ClearType};

use super::mode::Mode;
use super::{HELP, Pager};
use crate::search::SearchError;
use crate::text::ansi::Style;
use crate::text::{Highlight, WrapMode};
use crate::ui::{BarContent, StatusContext, display_width};

const SEP: &str = "/";
const SPACE_LEN: usize = 3;

impl Pager {
  /// Render the current frame to `out`.
  ///
  /// # Errors
  ///
  /// Returns an error if output fails or a coordinate cannot be represented as a `u16`.
  pub fn render(&mut self, out: &mut impl io::Write) -> anyhow::Result<()> {
    match &self.mode {
      Mode::Normal => self.render_normal(out),
      Mode::Help => self.render_help(out),
      Mode::Search(_) => self.render_search(out),
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

      self.highlight_buf.clear();
      self.collect_highlights(line);

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
          self
            .parsed_buf
            .render_range(out, window.range, &self.highlight_buf)?;
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
            self
              .parsed_buf
              .render_range(out, range, &self.highlight_buf)?;
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
    )?;
    hide_cursor(out)?;
    Ok(())
  }

  fn render_status(&mut self, out: &mut impl io::Write) -> anyhow::Result<()> {
    let title = &self.title;
    let lines = self.buffer.line_count();
    let reading = self.buffer.is_streaming();
    let percent = self.viewport.percent(self.top.line, lines);
    let notice = self
      .notice
      .as_ref()
      .filter(|n| n.top == self.top)
      .map_or("", |n| n.message());

    let context = StatusContext::new(title, lines, percent, reading, notice);
    let status_row = u16::try_from(self.viewport.last_row())?;
    self.opts.ui.render_bar(
      out,
      status_row,
      self.viewport.width(),
      &BarContent::Status(&context),
      &mut self.status_buf,
    )?;
    hide_cursor(out)?;
    Ok(())
  }

  fn render_search(&mut self, out: &mut impl io::Write) -> anyhow::Result<()> {
    let Mode::Search(prompt) = &self.mode else {
      return Ok(());
    };

    let mode = prompt.settings.mode.label();
    let case = prompt.settings.case.label();
    let query = prompt.query();
    let error = prompt.error.as_ref().map_or("", SearchError::message);

    let prefix_len = mode.len() + case.len() + SEP.len() + SPACE_LEN;
    let prompt_len = display_width(prompt.before_cursor());
    let width = (prefix_len + prompt_len).min(self.viewport.width().saturating_sub(1));
    let col = u16::try_from(width)?;
    let footer_row = u16::try_from(self.viewport.last_row())?;

    self.opts.ui.render_bar(
      out,
      footer_row,
      self.viewport.width(),
      &BarContent::Fmt(format_args!("{mode} {case} {SEP} {query} {error}")),
      &mut self.status_buf,
    )?;
    show_cursor(out, col, footer_row)?;
    Ok(())
  }

  fn collect_highlights(&mut self, line: usize) {
    let current_style = Style {
      fg: Some(self.opts.ui.theme.cur_match_fg),
      bg: Some(self.opts.ui.theme.cur_match_bg),
      attrs: Attributes::none(),
    };

    let normal_style = Style {
      fg: Some(self.opts.ui.theme.match_fg),
      bg: Some(self.opts.ui.theme.match_bg),
      attrs: Attributes::none(),
    };

    let current = self
      .search_state
      .cursor
      .as_ref()
      .filter(|c| c.hit.line == line)
      .map(|c| &c.hit.range);

    if let Some(matcher) = &self.search_state.matcher {
      self
        .highlight_buf
        .extend(matcher.find_iter(self.parsed_buf.text()).map(|range| {
          let style = if current == Some(&range) {
            current_style
          } else {
            normal_style
          };
          Highlight { range, style }
        }));
    }
  }
}

fn hide_cursor(out: &mut impl io::Write) -> anyhow::Result<()> {
  queue!(
    out,
    SetCursorStyle::DefaultUserShape,
    crossterm::cursor::Hide
  )?;
  Ok(())
}

fn show_cursor(out: &mut impl io::Write, col: u16, row: u16) -> anyhow::Result<()> {
  queue!(
    out,
    SetCursorStyle::BlinkingUnderScore,
    MoveTo(col, row),
    crossterm::cursor::Show,
  )?;
  Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
  use crate::pager::test_util::{set_query, test_pager};

  #[test]
  fn only_landed_on_match_gets_current_style() {
    let mut pager = test_pager("foo bar foo\nfoo\n", 80, 5);
    let theme = pager.opts.ui.theme;
    set_query(&mut pager, "foo");

    let collect = |pager: &mut crate::pager::Pager, line: usize| {
      pager.buffer.index_till_line(line + 1);
      let raw = pager.buffer.line_bytes(line).unwrap().to_vec();
      pager
        .parsed_buf
        .parse(&raw, pager.opts.ansi, pager.opts.tab_width);
      pager.highlight_buf.clear();
      pager.collect_highlights(line);
      pager
        .highlight_buf
        .iter()
        .map(|h| (h.range.clone(), h.style.bg))
        .collect::<Vec<_>>()
    };

    // before any search lands: every match is highlighted, none as current
    assert_eq!(
      collect(&mut pager, 0),
      vec![(0..3, Some(theme.match_bg)), (8..11, Some(theme.match_bg))]
    );

    // the first jump lands on 0..3
    pager.search_next();
    assert_eq!(
      collect(&mut pager, 0),
      vec![
        (0..3, Some(theme.cur_match_bg)),
        (8..11, Some(theme.match_bg))
      ]
    );

    // the same range on another line must not count as current
    assert_eq!(collect(&mut pager, 1), vec![(0..3, Some(theme.match_bg))]);
  }
}
