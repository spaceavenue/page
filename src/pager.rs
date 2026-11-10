mod mode;
mod notice;
mod prompt;
mod render;
mod search;

use std::io::{BufWriter, Write};
use std::time::{Duration, Instant};

use crossterm::event::KeyCode::{Backspace, Char, Delete, End, Enter, Esc, Home, Left, Right};
use crossterm::event::{Event, KeyEvent, KeyModifiers};
use crossterm::{event, terminal};

use crate::buffer::Buffer;
use crate::config::action::{Action, action_for_key};
use crate::config::options::Options;
use crate::pager::mode::Mode;
use crate::pager::notice::Notice;
use crate::pager::prompt::SearchPrompt;
use crate::pager::search::SearchState;
use crate::scroll::{self, Anchor};
use crate::search::Matcher;
use crate::text::{Highlight, ParsedLine, WrapMode};
use crate::ui::viewport::Viewport;

const STREAM_POLL_INTERVAL: Duration = Duration::from_millis(16);

const HELP: &str = r"page-rs keys,

  j / Down / Enter       line down
  k / Up                 line up
  Space / PgDn / C-f     page down
  PgUp / C-b             page up
  C-d / C-u              half page down/up
  g / Home               go to top
  G / End                go to bottom
  Left / Right           scroll horizontally (no wrap mode)
  0                      reset horizontal scroll
  /                      start searching
  C-c / Esc              stop searching (search mode)
  M-r                    toggle between regex and substring modes
  M-c                    cycle between smart -> sensitive -> insensitive case
  h / ?                  this help
  q / Esc / C-c          quit
";

pub struct Pager {
  buffer: Buffer,
  title: String,
  top: Anchor,
  viewport: Viewport,
  running: bool,
  dirty: bool,
  mode: Mode,
  left: usize,
  following: bool,
  opts: Options,
  search_state: SearchState,
  notice: Option<Notice>,
  parsed_buf: ParsedLine,
  highlight_buf: Vec<Highlight>,
  status_buf: String,
}

impl Pager {
  /// Create a new pager instance for `buffer` with `options`.
  ///
  /// # Errors
  ///
  /// Returns an error if querying the terminal dimensions fails.
  pub fn new(buffer: Buffer, title: String, options: Options) -> anyhow::Result<Self> {
    let (width, height) = terminal::size()?;
    let follow = options.initial_follow;
    let search_state = SearchState::new(options.search_settings);

    let mut pager = Self {
      buffer,
      title,
      top: Anchor::default(),
      viewport: Viewport::new(usize::from(width), usize::from(height)),
      running: true,
      dirty: true,
      mode: Mode::Normal,
      left: 0,
      following: follow,
      opts: options,
      search_state,
      notice: None,
      parsed_buf: ParsedLine::default(),
      highlight_buf: Vec::new(),
      status_buf: String::new(),
    };

    if pager.following {
      pager.jump_to_bottom();
    }

    Ok(pager)
  }

  /// Run the interactive pager event loop.
  ///
  /// # Errors
  ///
  /// Returns an error if terminal rendering or input polling fails.
  pub fn run(&mut self) -> anyhow::Result<()> {
    let mut stdout = BufWriter::with_capacity(64 * 1024, std::io::stdout().lock());

    while self.running {
      self.expire_notice(Instant::now());
      let got_data = self.buffer.read_from_stdin()?;
      if got_data {
        self.dirty();
        if self.following {
          self.jump_to_bottom();
        }
      }

      if self.dirty {
        self.render(&mut stdout)?;
        stdout.flush()?;
        self.clean();
      }
      let timeout = self.next_wakeup(Instant::now());
      let ready = timeout.map_or(Ok(true), event::poll)?;
      if ready {
        self.handle_event(event::read()?);
      }
    }

    Ok(())
  }

  fn next_wakeup(&self, now: Instant) -> Option<Duration> {
    if let Some(notice) = &self.notice {
      let duration = notice.expires.saturating_duration_since(now);
      if self.buffer.is_streaming() {
        return Some(duration.min(STREAM_POLL_INTERVAL));
      }
      return Some(duration);
    } else if self.buffer.is_streaming() {
      return Some(STREAM_POLL_INTERVAL);
    }
    None
  }

  const fn dirty(&mut self) {
    self.dirty = true;
  }

  const fn clean(&mut self) {
    self.dirty = false;
  }

  fn handle_event(&mut self, event: Event) {
    match event {
      Event::Key(key_event) => match &mut self.mode {
        Mode::Normal => self.handle_normal_key(key_event),
        Mode::Help => self.handle_help_key(key_event),
        Mode::Search(_) => self.handle_search_key(key_event),
      },
      Event::Resize(width, height) => {
        self.viewport = Viewport::new(usize::from(width), usize::from(height));
        // A row or column offset is meaningless after the size changes.
        self.top.row = 0;
        self.left = 0;
        self.set_top(self.top);
        self.dirty();
      },
      _ => {},
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

  fn handle_search_key(&mut self, key_event: KeyEvent) {
    let Mode::Search(prompt) = &mut self.mode else {
      return;
    };
    let ctrl = key_event.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key_event.modifiers.contains(KeyModifiers::ALT);
    match key_event.code {
      Esc => self.mode = Mode::Normal,
      Char('c') if alt && !ctrl => prompt.toggle_case(),
      Char('c') if ctrl => self.mode = Mode::Normal,
      Char('r') if alt && !ctrl => prompt.toggle_mode(),
      Char(ch) if !ctrl => prompt.insert(ch),
      Backspace => prompt.backspace(),
      Delete => prompt.delete(),
      Left => prompt.move_left(),
      Right => prompt.move_right(),
      Home => prompt.home(),
      End => prompt.end(),
      Enter => match Matcher::with_settings(prompt.query(), prompt.settings) {
        Ok(m) => {
          self.search_state.matcher = Some(m);
          self.search_state.reset();
          self.search_state.settings = prompt.settings;
          self.search_next();
          self.mode = Mode::Normal;
        },
        Err(e) => prompt.error = Some(e),
      },
      _ => {},
    }
    self.dirty();
  }

  fn dispatch(&mut self, action: Action) {
    use Action::{
      GotoBottom, GotoTop, HalfPageDown, HalfPageUp, Help, LineDown, LineUp, PageDown, PageUp,
      Quit, ScrollHome, ScrollLeft, ScrollRight, SearchBackward, SearchCaseToggle, SearchForward,
      SearchModeToggle, SearchStart,
    };

    let body = self.viewport.body_height();

    match action {
      LineUp => self.scroll_vertical(true, 1),
      LineDown => self.scroll_vertical(false, 1),
      PageUp => self.scroll_vertical(true, body),
      PageDown => self.scroll_vertical(false, body),
      HalfPageUp => self.scroll_vertical(true, (body / 2).max(1)),
      HalfPageDown => self.scroll_vertical(false, (body / 2).max(1)),
      GotoTop => {
        self.following = false;
        self.set_top(Anchor::default());
      },
      GotoBottom => self.jump_to_bottom(),
      ScrollLeft => self.scroll_horizontal(false),
      ScrollRight => self.scroll_horizontal(true),
      ScrollHome => self.left = 0,
      Help => self.mode = Mode::Help,
      Quit => self.running = false,
      SearchStart => self.mode = Mode::Search(SearchPrompt::new(self.search_state.settings)),
      SearchForward => {
        self.search_next();
      },
      SearchBackward => {
        self.search_prev();
      },
      SearchModeToggle => self.toggle_search_mode(),
      SearchCaseToggle => self.toggle_search_case(),
    }
    self.dirty();
  }

  fn scroll_vertical(&mut self, up: bool, rows: usize) {
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

  fn gutter_width(&self) -> usize {
    self
      .viewport
      .gutter_width(self.opts.line_numbers, self.buffer.line_count())
  }

  fn content_width(&self) -> usize {
    self.viewport.content_width(self.gutter_width())
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
      WrapMode::Soft | WrapMode::Hard => scratch.count_rows(self.content_width(), self.opts.wrap),
    }
  }
}

#[cfg(test)]
mod test_util;
#[cfg(test)]
mod tests;
