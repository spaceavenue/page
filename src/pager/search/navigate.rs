use std::ops::Range;

use crate::pager::Pager;
use crate::pager::notice::NoticeKind;
use crate::search::{SearchDirection, SearchHit};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchOutcome {
  NoMatcher,
  Found,
  NotFound,
  Wrapped,
}

/// Result of line-checking closure.
enum LineSearchRes {
  Match(Range<usize>),
  NoMatch,
  Eof,
}

impl Pager {
  /// Execute the search in `direction`.
  fn search(&mut self, direction: SearchDirection) -> SearchOutcome {
    let outcome = self.find_and_jump(direction);
    match outcome {
      SearchOutcome::NoMatcher | SearchOutcome::Found => self.notice = None,
      SearchOutcome::NotFound => self.set_notice(NoticeKind::NotFound),
      SearchOutcome::Wrapped => self.set_notice(NoticeKind::Wrapped),
    }
    outcome
  }

  /// Find and jump to next or previous match according to `direction`.
  fn find_and_jump(&mut self, direction: SearchDirection) -> SearchOutcome {
    let Some(matcher) = self.search_state.matcher.as_ref() else {
      return SearchOutcome::NoMatcher;
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
      return SearchOutcome::NotFound;
    };
    let wrapped = has_wrapped(&hit, (start_line, start_col), direction);
    self.jump_to_match(hit);
    if wrapped {
      return SearchOutcome::Wrapped;
    }
    SearchOutcome::Found
  }

  /// Find and jump to the next match.
  pub fn search_next(&mut self) -> SearchOutcome {
    self.search(SearchDirection::Forward)
  }

  /// Find and jump to the previous match.
  pub fn search_prev(&mut self) -> SearchOutcome {
    self.search(SearchDirection::Backward)
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

fn has_wrapped(hit: &SearchHit, start: (usize, usize), direction: SearchDirection) -> bool {
  let pos = (hit.line, hit.range.start);
  match direction {
    SearchDirection::Forward => pos < start,
    SearchDirection::Backward => pos >= start,
  }
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
  use super::SearchOutcome::{Found, NoMatcher, NotFound, Wrapped};
  use crate::pager::test_util::{numbered_doc, set_query, test_pager};
  use crate::scroll::Anchor;

  #[test]
  fn document_navigation_and_wrap_around() {
    let doc = "line 0\nline 1 target A\nline 2\nline 3 target B\n";
    let mut pager = test_pager(doc, 80, 2);
    set_query(&mut pager, "target");

    assert_eq!(pager.search_next(), Found);
    assert_eq!(pager.last_match().unwrap().line, 1);
    assert_eq!(pager.top.line, 1);

    assert_eq!(pager.search_next(), Found);
    assert_eq!(pager.last_match().unwrap().line, 3);
    assert_eq!(pager.top.line, 3);

    // wrap forward -> first match
    assert_eq!(pager.search_next(), Wrapped);
    assert_eq!(pager.last_match().unwrap().line, 1);

    // wrap backward -> last match
    assert_eq!(pager.search_prev(), Wrapped);
    assert_eq!(pager.last_match().unwrap().line, 3);

    assert_eq!(pager.search_prev(), Found);
    assert_eq!(pager.last_match().unwrap().line, 1);
  }

  #[test]
  fn intra_line_multiple_matches() {
    let mut pager = test_pager("first target and second target\n", 80, 2);
    set_query(&mut pager, "target");

    assert_eq!(pager.search_next(), Found);
    assert_eq!(pager.last_match().unwrap().range, 6..12);

    assert_eq!(pager.search_next(), Found);
    assert_eq!(pager.last_match().unwrap().range, 24..30);

    // wraps back to the first match on the same line
    assert_eq!(pager.search_next(), Wrapped);
    assert_eq!(pager.last_match().unwrap().range, 6..12);

    assert_eq!(pager.search_prev(), Wrapped);
    assert_eq!(pager.last_match().unwrap().range, 24..30);
  }

  #[test]
  fn only_match_reports_wrapped() {
    let mut pager = test_pager("nothing\nthe only target\nnothing\n", 80, 2);
    set_query(&mut pager, "target");

    // the first search cannot know the match is the only one
    assert_eq!(pager.search_next(), Found);
    let hit = pager.last_match().unwrap().clone();

    assert_eq!(pager.search_next(), Wrapped);
    assert_eq!(pager.last_match().unwrap(), &hit);
    assert_eq!(pager.search_prev(), Wrapped);
    assert_eq!(pager.last_match().unwrap(), &hit);
  }

  #[test]
  fn missing_or_uninitialized_search() {
    let mut pager = test_pager("hello world\n", 80, 2);
    assert_eq!(pager.search_next(), NoMatcher);
    assert_eq!(pager.search_prev(), NoMatcher);

    set_query(&mut pager, "missing");
    assert_eq!(pager.search_next(), NotFound);
    assert_eq!(pager.search_prev(), NotFound);
    assert_eq!(pager.top, Anchor::default());
    assert!(pager.last_match().is_none());
  }

  #[test]
  fn finds_matches_below_viewport() {
    let mut doc = numbered_doc(1000, &[], "");
    doc.push_str("needle here\n");
    let mut pager = test_pager(&doc, 80, 24);
    set_query(&mut pager, "needle");

    assert_eq!(pager.search_next(), Found);
    assert_eq!(pager.last_match().unwrap().line, 1000);

    // and back up again: from the top of the document, backward search wraps to the end
    pager.top = Anchor::default();
    assert_eq!(pager.search_prev(), Wrapped);
    assert_eq!(pager.last_match().unwrap().line, 1000);
  }

  #[test]
  fn scrolling_away_discards_search_cursor() {
    let doc = numbered_doc(40, &[1, 20], " target");
    let mut pager = test_pager(&doc, 80, 6);
    set_query(&mut pager, "target");

    assert_eq!(pager.search_next(), Found);
    assert_eq!(pager.last_match().unwrap().line, 1);

    // Scroll past the match. Searching backward must start from the new screen and find line 1
    // again. A stale cursor would continue from line 1 and wrap around to line 20.
    pager.scroll_vertical(false, 10);
    assert_eq!(pager.top.line, 10);
    assert_eq!(pager.search_prev(), Found);
    assert_eq!(pager.last_match().unwrap().line, 1);
  }
}
