#![allow(clippy::unwrap_used)]

use super::*;
use crate::pager::search::navigate::SearchOutcome::NotFound;
use crate::pager::test_util::{press, set_query, test_pager, type_text};
use crate::search::settings::{SearchCase, SearchMode};

#[test]
fn prompt_opens_in_current_mode() {
  let mut pager = test_pager("a\n", 80, 3);
  pager.search_state.settings.mode = SearchMode::Regex;

  pager.dispatch(Action::SearchStart);
  assert!(matches!(&pager.mode, Mode::Search(prompt) if prompt.settings.mode == SearchMode::Regex));
}

#[test]
fn prompt_mode_is_draft_until_enter() {
  let mut pager = test_pager("line one\nline (two\n", 80, 3);
  set_query(&mut pager, "line");

  // toggling at the prompt changes only the prompt: the committed state is untouched
  pager.dispatch(Action::SearchStart);
  press(&mut pager, Char('r'), KeyModifiers::ALT);
  assert!(matches!(&pager.mode, Mode::Search(prompt) if prompt.settings.mode == SearchMode::Regex));
  assert_eq!(pager.search_state.settings.mode, SearchMode::Substring);
  assert_eq!(
    pager.search_state.matcher.as_ref().unwrap().mode(),
    SearchMode::Substring
  );

  // Esc discards the draft
  press(&mut pager, Esc, KeyModifiers::NONE);
  assert_eq!(pager.mode, Mode::Normal);
  assert_eq!(pager.search_state.settings.mode, SearchMode::Substring);

  // Enter commits it, together with the new matcher
  pager.dispatch(Action::SearchStart);
  press(&mut pager, Char('r'), KeyModifiers::ALT);
  type_text(&mut pager, "t.o");
  press(&mut pager, Enter, KeyModifiers::NONE);
  assert_eq!(pager.mode, Mode::Normal);
  assert_eq!(pager.search_state.settings.mode, SearchMode::Regex);
  assert_eq!(
    pager.search_state.matcher.as_ref().unwrap().mode(),
    SearchMode::Regex
  );
  assert_eq!(pager.last_match().unwrap().line, 1);
}

#[test]
fn prompt_toggle_defers_validation_to_enter() {
  let mut pager = test_pager("a(b\n", 80, 3);
  pager.dispatch(Action::SearchStart);
  type_text(&mut pager, "a(");

  // "a(" is a fine literal but an invalid regex. The toggle does not judge it yet.
  press(&mut pager, Char('r'), KeyModifiers::ALT);
  assert!(matches!(&pager.mode, Mode::Search(p) if p.error.is_none() && p.query() == "a("));

  // Enter compiles it: the error appears and the prompt stays open for editing.
  press(&mut pager, Enter, KeyModifiers::NONE);
  assert!(matches!(&pager.mode, Mode::Search(p) if p.error.is_some()));
  assert!(pager.search_state.matcher.is_none());

  // toggling back drops the stale error, and the literal now commits
  press(&mut pager, Char('r'), KeyModifiers::ALT);
  assert!(matches!(&pager.mode, Mode::Search(p) if p.error.is_none()));
  press(&mut pager, Enter, KeyModifiers::NONE);
  assert_eq!(pager.mode, Mode::Normal);
  assert!(pager.search_state.matcher.is_some());
}

#[test]
fn prompt_case_is_draft_until_enter() {
  let mut pager = test_pager("Hello\n", 80, 3);
  set_query(&mut pager, "hello");

  // Esc discards the draft
  pager.dispatch(Action::SearchStart);
  press(&mut pager, Char('c'), KeyModifiers::ALT);
  assert!(
    matches!(&pager.mode, Mode::Search(p) if p.settings.case == SearchCase::Sensitive && p.query().is_empty()),
    "Alt-C must change the draft, not type a 'c'"
  );
  assert_eq!(pager.search_state.settings.case, SearchCase::Smart);
  press(&mut pager, Esc, KeyModifiers::NONE);
  assert_eq!(pager.search_state.settings.case, SearchCase::Smart);
  assert_eq!(
    pager.search_state.matcher.as_ref().unwrap().case(),
    SearchCase::Smart
  );

  // Enter commits it together with the new matcher
  pager.dispatch(Action::SearchStart);
  press(&mut pager, Char('c'), KeyModifiers::ALT);
  type_text(&mut pager, "hello");
  press(&mut pager, Enter, KeyModifiers::NONE);
  assert_eq!(pager.mode, Mode::Normal);
  assert_eq!(pager.search_state.settings.case, SearchCase::Sensitive);
  assert_eq!(
    pager.search_state.matcher.as_ref().unwrap().case(),
    SearchCase::Sensitive
  );
  assert_eq!(pager.search_next(), NotFound);
}
