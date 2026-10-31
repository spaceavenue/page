use std::io::{self, Write};
use std::time::Duration;

use crossterm::cursor::MoveTo;
use crossterm::event::Event;
use crossterm::style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor};
use crossterm::terminal::{Clear, ClearType};
use crossterm::{event, queue, terminal};

use crate::action::{Action, action_for_key};
use crate::buffer::Buffer;
use crate::scroll::{self, Anchor};
use crate::text::ParsedLine;

const STREAM_POLL_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Clone, Copy, Debug)]
pub struct Options {
  pub follow: bool,
  pub hard_wrap: bool,
  pub ansi: bool,
  pub tab_width: usize,
}

#[allow(clippy::struct_excessive_bools)]
pub struct Pager {
  buffer: Buffer,
  top: Anchor,
  width: usize,
  height: usize,
  running: bool,
  dirty: bool,
  opts: Options,
  parsed: ParsedLine,
}

impl Pager {
  /// Create a new pager instance for `buffer` with `options`.
  ///
  /// # Errors
  ///
  /// Returns an error if querying the terminal dimensions fails.
  pub fn new(buffer: Buffer, options: Options) -> anyhow::Result<Self> {
    let (width, height) = terminal::size()?;

    Ok(Self {
      buffer,
      top: Anchor::default(),
      width: usize::from(width),
      height: usize::from(height),
      running: true,
      dirty: true,
      opts: options,
      parsed: ParsedLine::default(),
    })
  }

  const fn body_height(&self) -> usize {
    self.height.saturating_sub(1)
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
        if self.opts.follow {
          self.jump_to_bottom();
        }
      }

      if self.dirty {
        self.render(&mut stdout)?;
        self.dirty = false;
      }

      if self.buffer.is_streaming() {
        if event::poll(STREAM_POLL_INTERVAL)? {
          self.handle_event(event::read()?);
        }
      } else {
        self.handle_event(event::read()?);
      }
    }

    Ok(())
  }

  fn handle_event(&mut self, event: Event) {
    match event {
      Event::Key(key_event) => {
        if let Some(action) = action_for_key(key_event) {
          self.dispatch(action);
        }
      }
      Event::Resize(width, height) => {
        self.width = usize::from(width);
        self.height = usize::from(height);
        // A row offset is meaningless after the width changes.
        self.top.row = 0;
        self.set_top(self.top);
        self.dirty = true;
      }
      _ => {}
    }
  }

  fn dispatch(&mut self, action: Action) {
    use Action::{
      GotoBottom, GotoTop, HalfPageDown, HalfPageUp, LineDown, LineUp, PageDown, PageUp, Quit,
    };

    let body = self.body_height();

    match action {
      LineUp => self.scroll_by(true, 1),
      LineDown => self.scroll_by(false, 1),
      PageUp => self.scroll_by(true, body),
      PageDown => self.scroll_by(false, body),
      HalfPageUp => self.scroll_by(true, (body / 2).max(1)),
      HalfPageDown => self.scroll_by(false, (body / 2).max(1)),
      GotoTop => {
        self.opts.follow = false;
        self.set_top(Anchor::default());
      }
      GotoBottom => self.jump_to_bottom(),
      Quit => self.running = false,
    }
    self.dirty = true;
  }

  fn scroll_by(&mut self, up: bool, rows: usize) {
    if up {
      self.opts.follow = false;
      self.top = self.rewind(self.top, rows);
    } else {
      let target = self.top.line + self.body_height() + rows;
      self.buffer.index_till_line(target);
      self.top = self.advance(self.top, rows);
    }

    self.clamp_if_at_end();
  }

  fn set_top(&mut self, top: Anchor) {
    self.buffer.index_till_line(top.line + self.body_height());
    self.top = top;
    self.clamp_if_at_end();
  }

  fn jump_to_bottom(&mut self) {
    self.buffer.index_till_line(usize::MAX);
    self.top = self.max_top();
    self.opts.follow = self.buffer.is_streaming();
  }

  fn clamp_if_at_end(&mut self) {
    if self.buffer.is_fully_indexed()
      && self.top.line + self.body_height() >= self.buffer.line_count()
    {
      self.top = self.top.min(self.max_top());
    }
  }

  fn max_top(&self) -> Anchor {
    let total = self.buffer.line_count();
    if total == 0 {
      return Anchor::default();
    }
    let last = total.saturating_sub(1);
    let mut scratch = ParsedLine::default();
    let last_row = self.line_rows(last, &mut scratch).saturating_sub(1);
    let end = Anchor {
      line: last,
      row: last_row,
    };
    self.rewind(end, self.body_height().saturating_sub(1))
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
    scratch.count_rows(self.width, !self.opts.hard_wrap)
  }

  fn render(&mut self, out: &mut impl Write) -> anyhow::Result<()> {
    self
      .buffer
      .index_till_line(self.top.line + self.body_height());

    let body = self.body_height();
    let mut position = self.top;
    let mut screen_row = 0usize;

    while screen_row < body {
      let Some(raw) = self.buffer.line_bytes(position.line) else {
        let cursor_row = u16::try_from(screen_row)?;
        queue!(
          out,
          MoveTo(0, cursor_row),
          Clear(ClearType::CurrentLine),
          SetForegroundColor(Color::Grey),
          Print("~"),
          ResetColor
        )?;
        screen_row += 1;
        position = Anchor {
          line: position.line + 1,
          row: 0,
        };
        continue;
      };

      let available = body - screen_row;

      self.parsed.parse(raw, self.opts.ansi, self.opts.tab_width);
      let rows = self
        .parsed
        .rows(self.width, !self.opts.hard_wrap)
        .skip(position.row)
        .take(available);

      for range in rows {
        let cursor_row = u16::try_from(screen_row)?;
        queue!(out, MoveTo(0, cursor_row), Clear(ClearType::CurrentLine))?;
        self.parsed.render_range(out, range)?;
        screen_row += 1;
      }

      position = Anchor {
        line: position.line + 1,
        row: 0,
      };
    }

    self.render_status(out)?;
    out.flush()?;
    Ok(())
  }

  fn render_status(&self, out: &mut impl Write) -> anyhow::Result<()> {
    // This is a separate function from `render` so it can use `?` on both I/O
    // and integer conversions.
    let total = self.buffer.line_count();
    let still_reading = self.buffer.is_streaming();
    let percent = if total == 0 {
      100
    } else {
      let current = self.top.line.min(total);
      let current_u64 = u64::try_from(current).unwrap_or(0);
      let total_u64 = u64::try_from(total).unwrap_or(1).max(1);
      let pct = current_u64.saturating_mul(100) / total_u64;
      usize::try_from(pct).unwrap_or(100).min(100)
    };

    let status = format!(
      "{} {}{} lines {}% {}",
      self.buffer.filename(),
      total,
      if still_reading { "+" } else { "" },
      percent,
      if still_reading { "[reading...]" } else { "" }
    );

    let status_row = u16::try_from(self.height.saturating_sub(1))?;

    queue!(
      out,
      MoveTo(0, status_row),
      Clear(ClearType::CurrentLine),
      SetForegroundColor(Color::Black),
      SetBackgroundColor(Color::DarkGrey),
      Print(format!("{status:<width$}", width = self.width)),
      ResetColor
    )?;
    Ok(())
  }
}
