use std::ops::Range;

use crate::pager::Pager;
use crate::pager::notice::NoticeKind;
use crate::search::{SearchDirection, SearchHit};
use crate::text::{plain_text, verbatim_block};

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
    // already scanned the whole document, so nothing could have changed
    if self.search_state.exhausted {
      return SearchOutcome::NotFound;
    }

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

    let block_bytes = self.search_state.block_bytes;
    let buffer = &mut self.buffer;
    let parsed_buf = &mut self.parsed_buf;
    let opts = &self.opts;

    let Some(hit) = ({
      let skipping = direction == SearchDirection::Forward && matcher.can_skip_blocks();
      // range of lines proven to have no matches
      let mut empty = 0..0;
      // lines whose block check failed. go line by line
      let mut no_block = 0..0;

      let check_line = |line, col| -> LineSearchRes {
        if empty.contains(&line) {
          return LineSearchRes::NoMatch;
        }
        buffer.index_till_line(line);
        if skipping
          && !no_block.contains(&line)
          && let Some((lines, bytes)) = buffer.block_bytes(line, block_bytes)
        {
          if verbatim_block(bytes).is_some_and(|t| matcher.find_iter(t).next().is_none()) {
            empty = lines;
            return LineSearchRes::NoMatch;
          }
          no_block = lines;
        }

        let Some(raw) = buffer.line_bytes(line) else {
          return LineSearchRes::Eof;
        };
        // most lines are plain text. skip parsing and search in place
        let text = plain_text(raw).unwrap_or_else(|| {
          parsed_buf.parse_line(raw, opts.ansi, opts.tab_width, line);
          parsed_buf.text()
        });
        matcher
          .search_in(text, col, direction)
          .map_or(LineSearchRes::NoMatch, LineSearchRes::Match)
      };
      match direction {
        SearchDirection::Forward => find_next(start_line, start_col, check_line),
        SearchDirection::Backward => find_prev(total_lines, start_line, start_col, check_line),
      }
    }) else {
      self.search_state.exhausted = self.buffer.is_fully_indexed();
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
  use crate::pager::Pager;
  use crate::pager::test_util::{at, numbered_doc, set_query, test_pager};
  use crate::scroll::Anchor;
  use crate::search::SearchHit;
  use crate::text::ParsedLine;

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

    pager.scroll_vertical(false, 10);
    assert_eq!(pager.top.line, 10);
    assert_eq!(pager.search_prev(), Found);
    assert_eq!(pager.last_match().unwrap().line, 1);
  }

  /// 40 lines whose raw bytes and parsed text disagree in the ways that could fool block skipping
  fn skip_stress_doc(trailing_newline: bool) -> String {
    let specials = [
      (0, "needle first"),
      (5, "caf\u{e9} needle \u{e9}"),
      (9, "\x1b[31mnee\x1b[0mdle split by an escape"),
      (14, "crlf needle\r"),
      (15, "NEEDLE shouting"),
      (20, "needle needle twice"),
      (21, "tab\tneedle"),
      (27, "caret \x01 char"),
      (30, "nee\u{85}dle with a c1 control, then needle"),
      (39, "needle last"),
    ];
    let mut lines: Vec<String> = (0..40)
      .map(|i| format!("filler {i} {}", "x".repeat(i % 3)))
      .collect();
    for (at, text) in specials {
      if let Some(line) = lines.get_mut(at) {
        *line = text.to_string();
      }
    }
    let mut doc = lines.join("\n");
    if trailing_newline {
      doc.push('\n');
    }
    doc
  }

  fn all_hits(pager: &Pager, doc: &str) -> Vec<SearchHit> {
    let matcher = pager.search_state.matcher.as_ref().unwrap();
    let mut parsed = ParsedLine::default();
    let mut hits = Vec::new();
    for (line, raw) in doc.split_inclusive('\n').enumerate() {
      parsed.parse(raw.as_bytes(), pager.opts.ansi, pager.opts.tab_width);
      hits.extend(
        matcher
          .find_iter(parsed.text())
          .map(|range| SearchHit { line, range }),
      );
    }
    hits
  }

  #[test]
  fn block_skipping_finds_exactly_what_line_by_line_scan_would() {
    for trailing_newline in [true, false] {
      let doc = skip_stress_doc(trailing_newline);
      // 1 makes every line its own block; 7 and 24 cut lines in awkward places; 64 lines up with
      // the chunk size of the byte scan; the last holds the whole document in one block
      for block_bytes in [1, 7, 24, 64, 100_000] {
        for query in ["needle", "^A", "x", "\u{e9}", "absent"] {
          let fresh = |start: usize| {
            let mut pager = test_pager(&doc, 80, 2);
            pager.search_state.block_bytes = block_bytes;
            set_query(&mut pager, query);
            if start > 0 {
              // the search clamps its start to the lines indexed so far
              pager.buffer.index_till_line(usize::MAX);
            }
            pager.top = at(start, 0);
            pager
          };
          let expected = all_hits(&fresh(0), &doc);

          for start in 0..40 {
            let ctx = format!(
              "query {query:?}, block {block_bytes}, start line {start}, trailing newline \
               {trailing_newline}"
            );
            let len = expected.len();
            if len == 0 {
              assert_eq!(fresh(start).search_next(), NotFound, "{ctx}");
              assert_eq!(fresh(start).search_prev(), NotFound, "{ctx}");
              continue;
            }

            let mut pager = fresh(start);
            let first = expected.partition_point(|hit| hit.line < start);
            for step in 0..len + 2 {
              assert_ne!(pager.search_next(), NotFound, "{ctx}, step {step}");
              assert_eq!(
                pager.last_match(),
                expected.get((first + step) % len),
                "forward: {ctx}, step {step}"
              );
            }

            let mut pager = fresh(start);
            let at_or_before = expected.partition_point(|hit| hit.line <= start);
            let first = at_or_before.checked_sub(1).unwrap_or(len - 1);
            for step in 0..len + 2 {
              assert_ne!(pager.search_prev(), NotFound, "{ctx}, step {step}");
              assert_eq!(
                pager.last_match(),
                expected.get((first + len - step % len) % len),
                "backward: {ctx}, step {step}"
              );
            }
          }
        }
      }
    }
  }
}
