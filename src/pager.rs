mod mode;
mod render;

use std::time::Duration;

use crossterm::event::{Event, KeyEvent};
use crossterm::{event, terminal};

use crate::buffer::Buffer;
use crate::config::action::{Action, action_for_key};
use crate::config::options::Options;
use crate::pager::mode::Mode;
use crate::scroll::{self, Anchor};
use crate::text::{ParsedLine, WrapMode};
use crate::ui::viewport::Viewport;

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
  mode: Mode,
  left: usize,
  following: bool,
  opts: Options,
  parsed_buf: ParsedLine,
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
      mode: Mode::Normal,
      left: 0,
      following: follow,
      opts: options,
      parsed_buf: ParsedLine::default(),
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

  const fn dirty(&mut self) {
    self.dirty = true;
  }

  const fn clean(&mut self) {
    self.dirty = false;
  }

  /// Run the interactive pager event loop.
  ///
  /// # Errors
  ///
  /// Returns an error if terminal rendering or input polling fails.
  pub fn run(&mut self) -> anyhow::Result<()> {
    let mut stdout = std::io::stdout().lock();

    while self.running {
      let got_data = self.buffer.read_from_stdin()?;
      if got_data {
        self.dirty();
        if self.following {
          self.jump_to_bottom();
        }
      }

      if self.dirty {
        self.render(&mut stdout)?;
        self.clean();
      }

      if !self.buffer.is_streaming() || event::poll(STREAM_POLL_INTERVAL)? {
        self.handle_event(event::read()?);
      }
    }

    Ok(())
  }

  fn handle_event(&mut self, event: Event) {
    match event {
      Event::Key(key_event) => {
        let mode = std::mem::replace(&mut self.mode, Mode::Normal);
        match mode {
          Mode::Normal => self.handle_normal_key(key_event),
          Mode::Help => self.handle_help_key(key_event),
        }
      }
      Event::Resize(width, height) => {
        self.viewport = Viewport::new(usize::from(width), usize::from(height));
        // A row or column offset is meaningless after the size changes.
        self.top.row = 0;
        self.left = 0;
        self.set_top(self.top);
        self.dirty();
      }
      _ => {}
    }
  }

  fn handle_normal_key(&mut self, key_event: KeyEvent) {
    if let Some(action) = action_for_key(key_event) {
      self.dispatch(action);
    }
  }

  fn handle_help_key(&mut self, key_event: KeyEvent) {
    if action_for_key(key_event) == Some(Action::Quit) {
      self.running = false;
    } else {
      self.mode = Mode::Normal;
    }
    self.dirty();
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
        self.following = false;
        self.set_top(Anchor::default());
      }
      GotoBottom => self.jump_to_bottom(),
      ScrollLeft => self.scroll_horizontal(false),
      ScrollRight => self.scroll_horizontal(true),
      ScrollHome => self.left = 0,
      Help => self.mode = Mode::Help,
      Quit => self.running = false,
    }
    self.dirty();
  }

  fn scroll_by(&mut self, up: bool, rows: usize) {
    if up {
      self.following = false;
      self.top = self.rewind(self.top, rows);
      self.clamp_if_at_end();
      return;
    }
    let target = self
      .viewport
      .bottom_line(self.top.line)
      .saturating_add(rows);
    self.buffer.index_till_line(target);
    self.top = self.advance(self.top, rows);

    self.clamp_if_at_end();
  }

  fn scroll_horizontal(&mut self, right: bool) {
    if self.opts.wrap != WrapMode::None {
      return;
    }

    if !right {
      self.left = self.left.saturating_sub(1);
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
    self.following = self.buffer.is_streaming();
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
}
