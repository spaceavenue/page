#![allow(clippy::unwrap_used)]

use super::search::SearchOutcome::{self, Found, NoMatcher, NotFound, Wrapped};
use super::*;
use crate::search::SearchMode;

fn test_pager(doc: &str, width: usize, height: usize) -> Pager {
  Pager {
    buffer: Buffer::from_bytes(doc.as_bytes()),
    title: "test".to_string(),
    top: Anchor::default(),
    viewport: Viewport::new(width, height),
    running: true,
    dirty: true,
    mode: Mode::Normal,
    left: 0,
    following: false,
    opts: Options::default(),
    search_state: SearchState::default(),
    parsed_buf: ParsedLine::default(),
    status_buf: String::new(),
  }
}

fn set_query(pager: &mut Pager, query: &str) {
  pager.search_state.matcher = Some(Matcher::new(query).unwrap());
  pager.search_state.cursor = None;
}

fn numbered_doc(count: usize, at: &[usize], extra: &str) -> String {
  use std::fmt::Write as _;
  let mut doc = String::new();
  for line in 0..count {
    let _ = write!(doc, "line {line}");
    if at.contains(&line) {
      doc.push_str(extra);
    }
    doc.push('\n');
  }
  doc
}

const fn at(line: usize, row: usize) -> Anchor {
  Anchor { line, row }
}

fn notice(pager: &Pager) -> Option<SearchOutcome> {
  pager.search_state.notice.map(|(outcome, _)| outcome)
}

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
fn visible_match_does_not_scroll() {
  let doc = numbered_doc(30, &[1, 3], " target");
  let mut pager = test_pager(&doc, 80, 6); // 5 body rows
  set_query(&mut pager, "target");

  assert_eq!(pager.search_next(), Found);
  assert_eq!(pager.last_match().unwrap().line, 1);
  assert_eq!(pager.top, Anchor::default());

  // the cursor must still advance even though the screen did not move
  assert_eq!(pager.search_next(), Found);
  assert_eq!(pager.last_match().unwrap().line, 3);
  assert_eq!(pager.top, Anchor::default());
}

#[test]
fn hidden_match_scrolls_margin() {
  let doc = numbered_doc(100, &[50], " target");
  let mut pager = test_pager(&doc, 80, 11); // 10 body rows
  set_query(&mut pager, "target");

  assert_eq!(pager.search_next(), Found);
  assert_eq!(pager.last_match().unwrap().line, 50);
  assert_eq!(pager.top, at(48, 0));
}

#[test]
fn margin_shrinks_in_small_viewports() {
  let doc = numbered_doc(100, &[50], " target");
  let mut pager = test_pager(&doc, 80, 2); // 1 body row
  set_query(&mut pager, "target");

  assert_eq!(pager.search_next(), Found);
  assert_eq!(pager.top, at(50, 0));
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
  pager.scroll_by(false, 10);
  assert_eq!(pager.top.line, 10);
  assert_eq!(pager.search_prev(), Found);
  assert_eq!(pager.last_match().unwrap().line, 1);
}

#[test]
fn wrapping_scrolls_and_moves_cursor() {
  let doc = numbered_doc(100, &[1, 50], " target");
  let mut pager = test_pager(&doc, 80, 11); // 10 body rows
  set_query(&mut pager, "target");

  assert_eq!(pager.search_next(), Found);
  assert_eq!(pager.search_next(), Found);
  assert_eq!(pager.last_match().unwrap().line, 50);
  assert_eq!(pager.top, at(48, 0));

  // line 1 is off screen: the wrap has to scroll back up to it
  assert_eq!(pager.search_next(), Wrapped);
  assert_eq!(pager.last_match().unwrap().line, 1);
  assert_eq!(pager.top, Anchor::default());

  // and the cursor moved with it, so the next search continues forward instead of wrapping again
  assert_eq!(pager.search_next(), Found);
  assert_eq!(pager.last_match().unwrap().line, 50);
}

#[test]
fn notice_follows_outcome_and_expires_on_scroll() {
  let doc = numbered_doc(40, &[1], " target");
  let mut pager = test_pager(&doc, 80, 6);
  assert_eq!(notice(&pager), None);

  set_query(&mut pager, "target");
  assert_eq!(pager.search_next(), Found);
  assert_eq!(pager.search_state.notice, Some((Found, pager.top)));

  // wrapping onto the same match: the notice must be stamped after the jump
  assert_eq!(pager.search_next(), Wrapped);
  assert_eq!(pager.search_state.notice, Some((Wrapped, pager.top)));

  // moving the screen leaves the notice behind: render shows it only while the anchor matches
  pager.scroll_by(false, 3);
  assert_ne!(
    pager.search_state.notice.map(|(_, top)| top),
    Some(pager.top)
  );

  set_query(&mut pager, "missing");
  assert_eq!(pager.search_next(), NotFound);
  assert_eq!(pager.search_state.notice, Some((NotFound, pager.top)));

  // a successful toggle invalidates the old notice
  set_query(&mut pager, "target");
  assert_eq!(pager.search_next(), Wrapped);
  pager.toggle_search_mode().unwrap();
  assert_eq!(notice(&pager), None);
}

#[test]
fn wrapped_match_fully_visible() {
  // The match spans bytes 15..21: the end of row 1 and the start of row 2 at width 10.
  let mut doc = numbered_doc(20, &[], "");
  doc.push_str("aaaaaaaaaabbbbbneedleccccc\n");
  // lines after the match, so the end of the document does not clamp the scroll position
  doc.push_str(&numbered_doc(10, &[], ""));
  let mut pager = test_pager(&doc, 10, 6); // 5 body rows
  pager.opts.wrap = WrapMode::Hard;
  set_query(&mut pager, "needle");

  assert_eq!(pager.search_next(), Found);
  let hit = pager.last_match().unwrap().clone();
  assert_eq!((hit.line, hit.range), (20, 15..21));

  // one row of margin above the first row of the match
  assert_eq!(pager.top, at(20, 0));
  let bottom = pager.advance(pager.top, 4);
  assert!(bottom >= at(20, 2));
}

#[test]
fn match_taller_than_screen_shows_first_row() {
  let mut doc = numbered_doc(20, &[], "");
  doc.push_str("aaaaaaaaaabbbbbneedleccccc\n");
  let mut pager = test_pager(&doc, 10, 2); // 1 body row
  pager.opts.wrap = WrapMode::Hard;
  set_query(&mut pager, "needle");

  assert_eq!(pager.search_next(), Found);
  assert_eq!(pager.top, at(20, 1));
}

#[test]
fn nowrap_scrolls_horizontally_to_match() {
  let doc = format!("{}needle{}\n", "x".repeat(50), "y".repeat(50));
  let mut pager = test_pager(&doc, 20, 3);
  pager.opts.wrap = WrapMode::None;
  set_query(&mut pager, "needle");

  assert_eq!(pager.search_next(), Found);
  // columns 50..56 with a 7 column margin: the window is 43..63
  assert_eq!(pager.left, 43);

  // already fully visible: stays put
  pager.search_state.cursor = None;
  assert_eq!(pager.search_next(), Found);
  assert_eq!(pager.left, 43);
}

#[test]
fn toggle_search_mode_persists_and_recompiles() {
  let mut pager = test_pager("a(b\n", 80, 2);
  assert_eq!(pager.search_state.mode, SearchMode::Substring);

  // no matcher yet: only the mode changes
  pager.toggle_search_mode().unwrap();
  assert_eq!(pager.search_state.mode, SearchMode::Regex);
  pager.toggle_search_mode().unwrap();
  assert_eq!(pager.search_state.mode, SearchMode::Substring);

  // "a(" is a fine literal but an invalid regex: the toggle fails and nothing changes
  set_query(&mut pager, "a(");
  assert!(pager.toggle_search_mode().is_err());
  assert_eq!(pager.search_state.mode, SearchMode::Substring);
  assert_eq!(
    pager.search_state.matcher.as_ref().unwrap().mode(),
    SearchMode::Substring
  );
}
