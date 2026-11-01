use std::io::{self, Write};
use std::time::Duration;

use crossterm::cursor::MoveTo;
use crossterm::event::Event;
use crossterm::style::{Print, ResetColor, SetBackgroundColor, SetForegroundColor};
use crossterm::terminal::{Clear, ClearType};
use crossterm::{event, queue, terminal};

use crate::action::{Action, action_for_key};
use crate::buffer::Buffer;
pub use crate::options::Options;
use crate::scroll::{self, Anchor};
use crate::terminal::{BeginSynchronizedUpdate, EndSynchronizedUpdate};
use crate::text::{ParsedLine, WrapMode};
use crate::ui::StatusContext;
use crate::viewport::Viewport;

const STREAM_POLL_INTERVAL: Duration = Duration::from_millis(50);

const HELP: &str = r"page-rs keys,

  j / Down / Enter       line down
  k / Up                 line up
  Space / PgDn / C-f     page down
  PgUp / C-b             page up
  C-d / C-u              half page down/up
  g / Home               go to top
  G / End                go to bottom
  Left / Right           scroll horizontally (chop mode)
  0                      reset horizontal scroll
  h / ?                  this help
  q / Esc / C-c          quit
";

#[allow(clippy::struct_excessive_bools)]
pub struct Pager {
  buffer: Buffer,
  top: Anchor,
  viewport: Viewport,
  running: bool,
  dirty: bool,
  showing_help: bool,
  left: usize,
  follow: bool,
  opts: Options,
  parsed: ParsedLine,
  status_buf: String,
}

impl Pager {
  /// Create a new pager instance for `buffer` with `options`.
  ///
  /// # Errors
  ///
  /// Returns an error if querying the terminal dimensions fails.
  pub fn new(buffer: Buffer, options: Options) -> anyhow::Result<Self> {
    let (width, height) = terminal::size()?;
    let follow = options.initial_follow;

    Ok(Self {
      buffer,
      top: Anchor::default(),
      viewport: Viewport::new(usize::from(width), usize::from(height)),
      running: true,
      dirty: true,
      showing_help: false,
      left: 0,
      follow,
      opts: options,
      parsed: ParsedLine::default(),
      status_buf: String::new(),
    })
  }

  fn gutter_width(&self) -> usize {
    self
      .viewport
      .gutter_width(self.opts.line_numbers, self.buffer.line_count())
  }

  fn content_width(&self) -> usize {
    self.viewport.content_width(self.gutter_width())
  }

  /// Run the interactive pager event loop.
  ///
  /// # Errors
  ///
  /// Returns an error if terminal rendering or input polling fails.
  pub fn run(&mut self) -> anyhow::Result<()> {
    let mut stdout = io::stdout();

    while self.running {
      let got_data = self.buffer.read_from_stdin()?;
      if got_data {
        self.dirty = true;
        if self.follow {
          self.jump_to_bottom();
        }
      }

      if self.dirty {
        self.render(&mut stdout)?;
        self.dirty = false;
      }

      if !self.buffer.is_streaming() || event::poll(STREAM_POLL_INTERVAL)? {
        self.handle_event(event::read()?);
      }
    }

    Ok(())
  }

  fn handle_event(&mut self, event: Event) {
    match event {
      Event::Key(key_event) if self.showing_help => {
        if action_for_key(key_event) == Some(Action::Quit) {
          self.running = false;
        } else {
          self.showing_help = false;
        }
        self.dirty = true;
      }
      Event::Key(key_event) => {
        if let Some(action) = action_for_key(key_event) {
          self.dispatch(action);
        }
      }
      Event::Resize(width, height) => {
        self.viewport = Viewport::new(usize::from(width), usize::from(height));
        // A row or column offset is meaningless after the size changes.
        self.top.row = 0;
        self.left = 0;
        self.set_top(self.top);
        self.dirty = true;
      }
      _ => {}
    }
  }

  fn dispatch(&mut self, action: Action) {
    use Action::{
      GotoBottom, GotoTop, HalfPageDown, HalfPageUp, Help, LineDown, LineUp, PageDown, PageUp,
      Quit, ScrollHome, ScrollLeft, ScrollRight,
    };

    let body = self.viewport.body_height();

    match action {
      LineUp => self.scroll_by(true, 1),
      LineDown => self.scroll_by(false, 1),
      PageUp => self.scroll_by(true, body),
      PageDown => self.scroll_by(false, body),
      HalfPageUp => self.scroll_by(true, (body / 2).max(1)),
      HalfPageDown => self.scroll_by(false, (body / 2).max(1)),
      GotoTop => {
        self.follow = false;
        self.set_top(Anchor::default());
      }
      GotoBottom => self.jump_to_bottom(),
      ScrollLeft => self.scroll_horizontal(false),
      ScrollRight => self.scroll_horizontal(true),
      ScrollHome => self.left = 0,
      Help => self.showing_help = true,
      Quit => self.running = false,
    }
    self.dirty = true;
  }

  fn scroll_by(&mut self, up: bool, rows: usize) {
    if up {
      self.follow = false;
      self.top = self.rewind(self.top, rows);
    } else {
      let target = self
        .viewport
        .bottom_line(self.top.line)
        .saturating_add(rows);
      self.buffer.index_till_line(target);
      self.top = self.advance(self.top, rows);
    }

    self.clamp_if_at_end();
  }

  fn scroll_horizontal(&mut self, right: bool) {
    if self.opts.wrap != WrapMode::None {
      return;
    }

    if !right {
      if self.left > 0 {
        self.left -= 1;
      }
      return;
    }

    self
      .buffer
      .index_till_line(self.viewport.bottom_line(self.top.line));
    let limit = self
      .max_visible_width()
      .saturating_sub(self.content_width());

    if limit > self.left {
      self.left += 1;
    }
  }

  fn max_visible_width(&self) -> usize {
    let mut scratch = ParsedLine::default();
    let end = self.viewport.bottom_line(self.top.line);
    let mut max = 0;

    for line in self.top.line..end {
      if let Some(raw) = self.buffer.line_bytes(line) {
        scratch.parse(raw, self.opts.ansi, self.opts.tab_width);
        max = max.max(scratch.display_width());
      }
    }

    max
  }

  fn set_top(&mut self, top: Anchor) {
    self
      .buffer
      .index_till_line(self.viewport.bottom_line(top.line));
    self.top = top;
    self.clamp_if_at_end();
  }

  fn jump_to_bottom(&mut self) {
    self.buffer.index_till_line(usize::MAX);
    self.top = self.max_top();
    self.follow = self.buffer.is_streaming();
  }

  fn clamp_if_at_end(&mut self) {
    if self.buffer.is_fully_indexed()
      && self.viewport.bottom_line(self.top.line) >= self.buffer.line_count()
    {
      self.top = self.top.min(self.max_top());
    }
  }

  fn max_top(&self) -> Anchor {
    let total = self.buffer.line_count();
    if total == 0 {
      return Anchor::default();
    }
    let last = total - 1;
    let mut scratch = ParsedLine::default();
    let last_row = self.line_rows(last, &mut scratch) - 1;
    let end = Anchor {
      line: last,
      row: last_row,
    };
    self.rewind(end, self.viewport.body_height().saturating_sub(1))
  }

  fn advance(&self, anchor: Anchor, rows: usize) -> Anchor {
    let mut scratch = ParsedLine::default();
    scroll::advance(anchor, rows, |line| self.line_rows(line, &mut scratch))
  }

  fn rewind(&self, anchor: Anchor, rows: usize) -> Anchor {
    let mut scratch = ParsedLine::default();
    scroll::rewind(anchor, rows, |line| self.line_rows(line, &mut scratch))
  }

  fn line_rows(&self, line: usize, scratch: &mut ParsedLine) -> usize {
    let Some(raw) = self.buffer.line_bytes(line) else {
      return 1;
    };

    scratch.parse(raw, self.opts.ansi, self.opts.tab_width);
    match self.opts.wrap {
      WrapMode::None => 1,
      WrapMode::Soft => scratch.count_rows(self.content_width(), true),
      WrapMode::Hard => scratch.count_rows(self.content_width(), false),
    }
  }

  fn render(&mut self, out: &mut impl Write) -> anyhow::Result<()> {
    queue!(out, BeginSynchronizedUpdate)?;

    if self.showing_help {
      self.render_help(out)?;
      queue!(out, EndSynchronizedUpdate)?;
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
    queue!(out, EndSynchronizedUpdate)?;
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
