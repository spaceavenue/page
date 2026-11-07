use std::ops::Range;

use unicode_width::UnicodeWidthStr;

use crate::pager::Pager;
use crate::scroll::Anchor;
use crate::search::{
  CaseSensitivity, Matcher, SearchDirection, SearchError, SearchHit, SearchMode,
};
use crate::text::WrapMode;

/// Rows of context kept above a match when a jump has to scroll.
const SCROLL_MARGIN_ROWS: usize = 2;
/// Columns of context kept left of a match when a jump has to scroll horizontally.
const SCROLL_MARGIN_COLS: usize = 8;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchPrompt {
  pub query: String,
  pub error: Option<SearchError>,
}

/// A snapshot of where the last search was.
///
/// `n` / `N` continue from `hit` only while the screen is still at `top`. On scrolling, the search
/// restarts from the visible screen instead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchCursor {
  pub hit: SearchHit,
  pub top: Anchor,
}

#[derive(Clone, Debug, Default)]
pub struct SearchState {
  pub matcher: Option<Matcher>,
  pub mode: SearchMode,
  pub case_sensitivity: CaseSensitivity,
  pub cursor: Option<SearchCursor>,
}

impl SearchState {
  /// New search state that starts in `mode` with `case_sensitivity`.
  #[must_use]
  pub fn new(mode: SearchMode, case_sensitivity: CaseSensitivity) -> Self {
    Self {
      mode,
      case_sensitivity,
      ..Self::default()
    }
  }
}

/// Result of line-checking closure.
enum LineSearchRes {
  Match(Range<usize>),
  NoMatch,
  Eof,
}

impl Pager {
  /// The match the last search landed on.
  #[must_use]
  #[cfg_attr(not(test), allow(dead_code))]
  pub fn last_match(&self) -> Option<&SearchHit> {
    self.search_state.cursor.as_ref().map(|cursor| &cursor.hit)
  }

  /// Switch between literal and regex search.
  ///
  /// # Errors
  ///
  /// Returns an error if the current query is not valid in the new mode.
  pub fn toggle_search_mode(&mut self) -> Result<(), SearchError> {
    let mode = match self.search_state.mode {
      SearchMode::Substring => SearchMode::Regex,
      SearchMode::Regex => SearchMode::Substring,
    };
    if let Some(matcher) = self.search_state.matcher.as_mut() {
      matcher.with_mode(mode)?;
      // The old snapshots mean nothing to us.
      self.search_state.cursor = None;
    }
    self.search_state.mode = mode;
    Ok(())
  }

  /// Find and jump to the next or previous match in the document according to `direction`.
  fn search(&mut self, direction: SearchDirection) -> bool {
    let Some(matcher) = self.search_state.matcher.as_ref() else {
      return false;
    };

    // only continue from the last hit while the screen is exactly where that jump left it.
    let cursor = self
      .search_state
      .cursor
      .as_ref()
      .filter(|cursor| cursor.top == self.top);

    // this is specifically for the case where backward search needs to wrap from EOF. probably
    // better way to do this?
    if matches!(direction, SearchDirection::Backward) {
      self.buffer.index_till_line(usize::MAX);
    }
    let total_lines = self.buffer.line_count();
    let top_line = self.top.line.min(total_lines.saturating_sub(1));

    let (start_line, start_col) = match (direction, cursor) {
      (SearchDirection::Forward, Some(cursor)) => (cursor.hit.line, cursor.hit.range.end),
      (SearchDirection::Forward, None) => (top_line, 0),
      (SearchDirection::Backward, Some(cursor)) => (cursor.hit.line, cursor.hit.range.start),
      (SearchDirection::Backward, None) => (top_line, usize::MAX),
    };

    let buffer = &mut self.buffer;
    let parsed_buf = &mut self.parsed_buf;
    let opts = &self.opts;

    let Some(hit) = ({
      let check_line = |line, col| -> LineSearchRes {
        buffer.index_till_line(line);
        let Some(raw) = buffer.line_bytes(line) else {
          return LineSearchRes::Eof;
        };
        parsed_buf.parse(raw, opts.ansi, opts.tab_width);
        matcher
          .search_in(parsed_buf.text(), col, direction)
          .map_or(LineSearchRes::NoMatch, LineSearchRes::Match)
      };
      match direction {
        SearchDirection::Forward => find_next(start_line, start_col, check_line),
        SearchDirection::Backward => find_prev(total_lines, start_line, start_col, check_line),
      }
    }) else {
      return false;
    };
    self.jump_to_match(hit);
    true
  }

  /// Find and jump to the next match.
  pub fn search_next(&mut self) -> bool {
    self.search(SearchDirection::Forward)
  }

  /// Find and jump to the previous match.
  pub fn search_prev(&mut self) -> bool {
    self.search(SearchDirection::Backward)
  }

  /// Sets hit in pager, scrolling only if it is not already fully on screen, then remember it as
  /// the search cursor.
  fn jump_to_match(&mut self, hit: SearchHit) {
    self.following = false;
    if let Some(raw) = self.buffer.line_bytes(hit.line) {
      self
        .parsed_buf
        .parse(raw, self.opts.ansi, self.opts.tab_width);
      match self.opts.wrap {
        WrapMode::None => self.reveal_nowrap(&hit),
        WrapMode::Soft | WrapMode::Hard => self.reveal_wrapped(&hit),
      }
    }
    self.search_state.cursor = Some(SearchCursor { hit, top: self.top });
    self.dirty();
  }

  /// In `text::WrapMode::Soft` and `text::WrapMode::Hard` scroll vertically so every row of match
  /// is on screen.
  fn reveal_wrapped(&mut self, hit: &SearchHit) {
    let body = self.viewport.body_height();
    let width = self.content_width();
    let last_byte = hit.range.end.saturating_sub(1).max(hit.range.start);

    // soft wrapping can drop whitespace at a break. compare against each row's `end`.
    let mut start_row = None;
    let mut end_row = 0;
    for (index, range) in self.parsed_buf.rows(width, self.opts.wrap).enumerate() {
      end_row = index;
      if start_row.is_none() && hit.range.start < range.end {
        start_row = Some(index);
      }
      if last_byte < range.end {
        break;
      }
    }
    let start_row = start_row.unwrap_or(end_row);
    let span = end_row.saturating_sub(start_row) + 1;

    let first = Anchor {
      line: hit.line,
      row: start_row,
    };
    // match taller than the screen can never be fully visible. settle for its first row.
    let fits = span <= body;
    let last = if fits {
      Anchor {
        line: hit.line,
        row: end_row,
      }
    } else {
      first
    };

    // `advance` needs the rows to be indexed or it counts them as one row.
    self
      .buffer
      .index_till_line(self.viewport.bottom_line(self.top.line));
    let bottom = self.advance(self.top, body.saturating_sub(1));
    if self.top <= first && last <= bottom {
      return;
    }

    let margin = if fits {
      SCROLL_MARGIN_ROWS.min(body.saturating_sub(span) / 2)
    } else {
      0
    };
    self.set_top(self.rewind(first, margin));
  }

  /// In `text::WrapMode::None` scroll vertically to the hit line and horizontally to the match
  /// columns.
  fn reveal_nowrap(&mut self, hit: &SearchHit) {
    let body = self.viewport.body_height();
    let width = self.content_width();
    let visible_lines = self.top.line..self.viewport.bottom_line(self.top.line);
    if !visible_lines.contains(&hit.line) {
      let margin = SCROLL_MARGIN_ROWS.min(body.saturating_sub(1) / 2);
      self.set_top(Anchor {
        line: hit.line.saturating_sub(margin),
        row: 0,
      });
    }

    let text = self.parsed_buf.text();
    let start_col = UnicodeWidthStr::width(text.get(..hit.range.start).unwrap_or_default());
    let end_col = UnicodeWidthStr::width(text.get(..hit.range.end).unwrap_or_default());
    let span = end_col.saturating_sub(start_col);

    // match wider than the window can't be fully shown. settle for its first column.
    let fits = span <= width;
    let needed_end = if fits { end_col } else { start_col + 1 };
    if start_col >= self.left && needed_end <= self.left + width {
      return;
    }

    let margin = if fits {
      SCROLL_MARGIN_COLS.min(width.saturating_sub(span) / 2)
    } else {
      0
    };
    self.left = start_col.saturating_sub(margin);
  }
}

fn scan<T, F>(lines: T, start_col: usize, mut check_line: F) -> Option<SearchHit>
where
  T: Iterator<Item = usize>,
  F: FnMut(usize, usize) -> LineSearchRes,
{
  lines
    .map_while(|line| match check_line(line, start_col) {
      LineSearchRes::Eof => None,
      res => Some((line, res)),
    })
    .find_map(|(line, res)| match res {
      LineSearchRes::Match(range) => Some(SearchHit { line, range }),
      _ => None,
    })
}

/// Find the next match in a document using a line-match closure.
///
/// `check_line` is invoked with `(line_index, from_col)` and returns the byte range of the first
/// match.
fn find_next<F>(start_line: usize, start_col: usize, mut check_line: F) -> Option<SearchHit>
where
  F: FnMut(usize, usize) -> LineSearchRes,
{
  scan(start_line..=start_line, start_col, &mut check_line) // starting line, start col -> end
    .or_else(|| scan(start_line + 1.., 0, &mut check_line)) // everything till EOF
    .or_else(|| scan(0..start_line, 0, &mut check_line)) // wrap around, beinning to start line
    .or_else(|| scan(start_line..=start_line, 0, &mut check_line)) // beginning of start line -> start col
}

/// Find the previous match in a document using a reverse line-match closure.
///
/// `check_line` is invoked with `(line_index, before_col)` and returns the byte range of the last
/// match before `before_col`.
fn find_prev<F>(
  total_lines: usize,
  start_line: usize,
  start_col: usize,
  mut check_line: F,
) -> Option<SearchHit>
where
  F: FnMut(usize, usize) -> LineSearchRes,
{
  let max = usize::MAX;
  scan(start_line..=start_line, start_col, &mut check_line) // starting line, start col -> start of line
    .or_else(|| scan((0..start_line).rev(), max, &mut check_line)) // start line -> beginning of file
    .or_else(|| scan((start_line + 1..total_lines).rev(), max, &mut check_line)) // wrap around, EOF -> start line
    .or_else(|| scan(start_line..=start_line, max, &mut check_line)) // end of line -> start col
}
