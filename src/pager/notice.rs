use std::time::{Duration, Instant};

use crate::pager::Pager;
use crate::scroll::Anchor;
use crate::search::settings::{SearchCase, SearchMode};

/// Expiry time for a notice.
const NOTICE_TTL: Duration = Duration::from_secs(3);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NoticeKind {
  NotFound,
  Wrapped,
  ModeChanged(SearchMode),
  CaseChanged(SearchCase),
  ModeRejected(Box<str>),
}

/// A transient status-bar message. Shown while the screen is still at `top` and until `expires`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
  pub kind: NoticeKind,
  pub top: Anchor,
  pub expires: Instant,
}

impl Notice {
  pub fn message(&self) -> &str {
    match &self.kind {
      NoticeKind::NotFound => "[search]: no match found",
      NoticeKind::Wrapped => "[search]: wrapped around document",
      NoticeKind::ModeChanged(mode) => match mode {
        SearchMode::Substring => "[search]: substring mode",
        SearchMode::Regex => "[search]: regex mode",
      },
      NoticeKind::CaseChanged(case) => match case {
        SearchCase::Smart => "[search]: search uses smart case",
        SearchCase::Insensitive => "[search]: search is case insensitive",
        SearchCase::Sensitive => "[search]: search is case sensitive",
      },
      NoticeKind::ModeRejected(e) => e,
    }
  }
}

impl Pager {
  /// Show `kind` in the status bar until it times out or the screen moves.
  pub(super) fn set_notice(&mut self, kind: NoticeKind) {
    self.notice = Some(Notice {
      kind,
      top: self.top,
      expires: Instant::now() + NOTICE_TTL,
    });
  }

  /// Drop the notice once it has expired as of `now`.
  pub(super) fn expire_notice(&mut self, now: Instant) {
    if self.notice.as_ref().is_some_and(|n| n.expires <= now) {
      self.notice = None;
      self.dirty();
    }
  }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
  use std::time::{Duration, Instant};

  use super::*;
  use crate::pager::search::navigate::SearchOutcome::{Found, NotFound, Wrapped};
  use crate::pager::test_util::{notice, numbered_doc, set_query, test_pager};
  use crate::search::settings::SearchMode;

  #[test]
  fn notice_follows_outcome_and_expires_on_scroll() {
    let doc = numbered_doc(40, &[1, 3], " target");
    let mut pager = test_pager(&doc, 80, 6);
    assert_eq!(notice(&pager), None);

    set_query(&mut pager, "target");
    // plain jumps leave no notice (and so arm no timer)
    assert_eq!(pager.search_next(), Found);
    assert_eq!(pager.search_next(), Found);
    assert_eq!(notice(&pager), None);

    // wrapping: the notice must be stamped after the jump
    assert_eq!(pager.search_next(), Wrapped);
    let stamped = pager.notice.as_ref().unwrap();
    assert_eq!(stamped.kind, NoticeKind::Wrapped);
    assert_eq!(stamped.top, pager.top);

    // the stale "wrapped" message must not outlive the next plain jump
    assert_eq!(pager.search_next(), Found);
    assert_eq!(notice(&pager), None);

    // moving the screen leaves the notice behind: render shows it only while the anchor matches
    assert_eq!(pager.search_next(), Wrapped);
    pager.scroll_vertical(false, 3);
    assert_ne!(pager.notice.as_ref().unwrap().top, pager.top);

    // a successful toggle replaces the old notice with its own confirmation
    pager.toggle_search_mode();
    assert_eq!(
      notice(&pager),
      Some(NoticeKind::ModeChanged(SearchMode::Regex))
    );

    set_query(&mut pager, "missing");
    assert_eq!(pager.search_next(), NotFound);
    let stamped = pager.notice.as_ref().unwrap();
    assert_eq!(stamped.kind, NoticeKind::NotFound);
    assert_eq!(stamped.top, pager.top);
  }

  #[test]
  fn notice_timing() {
    let doc = numbered_doc(40, &[1], " target");
    let mut pager = test_pager(&doc, 80, 6);

    // nothing pending and nothing streaming: block until a key arrives
    assert_eq!(pager.next_wakeup(Instant::now()), None);

    set_query(&mut pager, "target");
    assert_eq!(pager.search_next(), Found);
    assert_eq!(pager.search_next(), Wrapped);
    let expires = pager.notice.as_ref().unwrap().expires;
    pager.clean();

    // one wake-up exactly at the deadline, not a poll loop
    let two_seconds_early = expires.checked_sub(Duration::from_secs(2)).unwrap();
    assert_eq!(
      pager.next_wakeup(two_seconds_early),
      Some(Duration::from_secs(2))
    );
    assert_eq!(pager.next_wakeup(expires), Some(Duration::ZERO));

    // just before the deadline: still there, nothing to redraw
    pager.expire_notice(expires.checked_sub(Duration::from_millis(1)).unwrap());
    assert!(pager.notice.is_some());
    assert!(!pager.dirty);

    pager.expire_notice(expires);
    assert!(pager.notice.is_none());
    assert!(pager.dirty);
  }
}
