#![allow(clippy::unwrap_used)]

use crossterm::event::KeyCode;

use crate::pager::notice::NoticeKind;
use crate::pager::*;

pub fn test_pager(doc: &str, width: usize, height: usize) -> Pager {
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
    notice: None,
    parsed_buf: ParsedLine::default(),
    status_buf: String::new(),
  }
}

pub fn set_query(pager: &mut Pager, query: &str) {
  pager.search_state.matcher = Some(Matcher::new(query).unwrap());
  pager.search_state.cursor = None;
}

pub fn numbered_doc(count: usize, at: &[usize], extra: &str) -> String {
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

pub const fn at(line: usize, row: usize) -> Anchor {
  Anchor { line, row }
}

pub fn notice(pager: &Pager) -> Option<NoticeKind> {
  pager.notice.as_ref().map(|n| n.kind.clone())
}

pub fn press(pager: &mut Pager, code: KeyCode, modifiers: KeyModifiers) {
  pager.handle_search_key(KeyEvent::new(code, modifiers));
}

pub fn type_text(pager: &mut Pager, text: &str) {
  for ch in text.chars() {
    press(pager, Char(ch), KeyModifiers::NONE);
  }
}
