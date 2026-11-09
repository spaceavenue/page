use crate::pager::Pager;
use crate::pager::notice::NoticeKind;
use crate::scroll::Anchor;
use crate::search::settings::SearchSettings;
use crate::search::{Matcher, SearchError, SearchHit};

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
  pub settings: SearchSettings,
  pub cursor: Option<SearchCursor>,
}

impl SearchState {
  /// New search state that starts with `settings`.
  #[must_use]
  pub fn new(settings: SearchSettings) -> Self {
    Self {
      settings,
      ..Self::default()
    }
  }
}

impl Pager {
  /// The match the last search landed on.
  #[must_use]
  #[cfg_attr(not(test), allow(dead_code))]
  pub fn last_match(&self) -> Option<&SearchHit> {
    self.search_state.cursor.as_ref().map(|cursor| &cursor.hit)
  }

  /// Switch between literal and regex search.
  pub fn toggle_search_mode(&mut self) {
    match self.switch_mode() {
      Ok(()) => self.set_notice(NoticeKind::ModeChanged(self.search_state.settings.mode)),
      Err(e) => self.set_notice(NoticeKind::ModeRejected(format!("[search]: {e}").into())),
    }
  }

  fn switch_mode(&mut self) -> Result<(), SearchError> {
    let mode = self.search_state.settings.mode.toggled();
    if let Some(matcher) = self.search_state.matcher.as_mut() {
      matcher.with_mode(mode)?;
      // The old snapshot means nothing to us.
      self.search_state.cursor = None;
    }
    self.search_state.settings.mode = mode;
    Ok(())
  }

  pub fn toggle_search_case(&mut self) {
    if self.switch_case() == Ok(()) {
      self.set_notice(NoticeKind::CaseChanged(self.search_state.settings.case));
    }
  }

  fn switch_case(&mut self) -> Result<(), SearchError> {
    let case = self.search_state.settings.case.next();
    if let Some(matcher) = self.search_state.matcher.as_mut() {
      matcher.with_case(case)?;
      self.search_state.cursor = None;
    }
    self.search_state.settings.case = case;
    Ok(())
  }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
  use crate::config::action::Action;
  use crate::pager::notice::NoticeKind;
  use crate::pager::search::navigate::SearchOutcome::{Found, NotFound};
  use crate::pager::test_util::{notice, set_query, test_pager};
  use crate::search::settings::{SearchCase, SearchMode};

  #[test]
  fn toggle_search_mode_persists_and_recompiles() {
    let mut pager = test_pager("a(b\n", 80, 2);
    assert_eq!(pager.search_state.settings.mode, SearchMode::Substring);

    // no matcher yet: only the mode changes
    pager.toggle_search_mode();
    assert_eq!(pager.search_state.settings.mode, SearchMode::Regex);
    assert_eq!(
      notice(&pager),
      Some(NoticeKind::ModeChanged(SearchMode::Regex))
    );
    pager.toggle_search_mode();
    assert_eq!(pager.search_state.settings.mode, SearchMode::Substring);
    assert_eq!(
      notice(&pager),
      Some(NoticeKind::ModeChanged(SearchMode::Substring))
    );

    // "a(" is a fine literal but an invalid regex: the toggle is rejected, nothing changes, and
    // the reason is surfaced in the notice
    set_query(&mut pager, "a(");
    pager.toggle_search_mode();
    assert_eq!(pager.search_state.settings.mode, SearchMode::Substring);
    assert_eq!(
      pager.search_state.matcher.as_ref().unwrap().mode(),
      SearchMode::Substring
    );
    assert!(matches!(
      notice(&pager),
      Some(NoticeKind::ModeRejected(message)) if message.starts_with("[search]: invalid regex")
    ));
  }

  #[test]
  fn toggle_search_case_recompiles_matcher() {
    let mut pager = test_pager("Hello\n", 80, 3);
    set_query(&mut pager, "hello");
    assert_eq!(pager.search_next(), Found);

    // smart -> sensitive: the lowercase query must stop matching "Hello"
    pager.dispatch(Action::SearchCaseToggle);
    assert_eq!(pager.search_state.settings.case, SearchCase::Sensitive);
    assert_eq!(
      pager.search_state.matcher.as_ref().unwrap().case(),
      SearchCase::Sensitive
    );
    assert_eq!(
      notice(&pager),
      Some(NoticeKind::CaseChanged(SearchCase::Sensitive))
    );
    assert_eq!(pager.search_next(), NotFound);

    // sensitive -> insensitive: it matches again
    pager.dispatch(Action::SearchCaseToggle);
    assert_eq!(pager.search_state.settings.case, SearchCase::Insensitive);
    assert_eq!(pager.search_next(), Found);
  }
}
