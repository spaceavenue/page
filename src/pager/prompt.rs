use unicode_segmentation::UnicodeSegmentation;

use crate::search::SearchError;
use crate::search::settings::SearchSettings;

/// The search prompt typed by the user.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchPrompt {
  query: String,
  cursor: usize,
  pub settings: SearchSettings,
  pub error: Option<SearchError>,
}

impl SearchPrompt {
  /// Create an empty prompt with `settings`.
  #[must_use]
  pub fn new(settings: SearchSettings) -> Self {
    Self {
      settings,
      ..Self::default()
    }
  }

  /// Switch the draft between literal and regex.
  pub fn toggle_mode(&mut self) {
    self.settings.mode = self.settings.mode.toggled();
    self.error = None;
  }

  /// Cycle the case-sensitivity of the draft.
  pub const fn toggle_case(&mut self) {
    self.settings.case = self.settings.case.next();
  }

  /// Returns the query in the prompt.
  pub fn query(&self) -> &str {
    &self.query
  }

  /// Returns the query until the cursor.
  pub fn before_cursor(&self) -> &str {
    self.query.get(..self.cursor).unwrap_or_default()
  }

  /// Insert a character at current cursor location.
  pub fn insert(&mut self, ch: char) {
    self.query.insert(self.cursor, ch);
    self.cursor += ch.len_utf8();
    self.error = None;
  }

  /// Delete the character before the cursor.
  pub fn backspace(&mut self) {
    let start = self.prev_boundary();
    self.query.drain(start..self.cursor);
    self.cursor = start;
    self.error = None;
  }

  /// Delete the character after the cursor.
  pub fn delete(&mut self) {
    let end = self.next_boundary();
    self.query.drain(self.cursor..end);
    self.error = None;
  }

  /// Move the cursor left.
  pub fn move_left(&mut self) {
    self.cursor = self.prev_boundary();
  }

  /// Move the cursor right.
  pub fn move_right(&mut self) {
    self.cursor = self.next_boundary();
  }

  /// Move the cursor to the beginning of the query.
  pub const fn home(&mut self) {
    self.cursor = 0;
  }

  /// Move the cursor to the end of the query.
  pub const fn end(&mut self) {
    self.cursor = self.query.len();
  }

  fn prev_boundary(&self) -> usize {
    self
      .query
      .get(..self.cursor)
      .and_then(|s| s.grapheme_indices(true).next_back())
      .map_or(0, |(i, _)| i)
  }

  fn next_boundary(&self) -> usize {
    self.cursor
      + self
        .query
        .get(self.cursor..)
        .and_then(|s| s.graphemes(true).next())
        .map_or(0, str::len)
  }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
  use super::*;
  use crate::search::Matcher;

  fn typed(text: &str) -> SearchPrompt {
    let mut prompt = SearchPrompt::default();
    for ch in text.chars() {
      prompt.insert(ch);
    }
    prompt
  }

  fn with_error(mut prompt: SearchPrompt) -> SearchPrompt {
    prompt.error = Some(Matcher::new("").unwrap_err());
    prompt
  }

  #[test]
  fn insert_goes_at_cursor() {
    let mut prompt = typed("abc");
    prompt.move_left();
    prompt.insert('x');
    assert_eq!(prompt.query(), "abxc");
    assert_eq!(prompt.before_cursor(), "abx");
  }

  #[test]
  fn edges_home_end_and_delete() {
    let mut prompt = typed("ab");
    prompt.home();
    prompt.backspace();
    prompt.move_left();
    assert_eq!((prompt.query(), prompt.before_cursor()), ("ab", ""));

    prompt.end();
    prompt.delete();
    prompt.move_right();
    assert_eq!((prompt.query(), prompt.before_cursor()), ("ab", "ab"));

    prompt.home();
    prompt.delete();
    assert_eq!((prompt.query(), prompt.before_cursor()), ("b", ""));
    prompt.end();
    assert_eq!(prompt.before_cursor(), "b");
  }

  #[test]
  fn moves_and_deletes_by_grapheme() {
    // `e` + combining acute accent is two code points but one grapheme
    let mut prompt = typed("ae\u{301}");
    prompt.backspace();
    assert_eq!(prompt.query(), "a");

    let mut prompt = typed("e\u{301}a");
    prompt.home();
    prompt.move_right();
    assert_eq!(prompt.before_cursor(), "e\u{301}");
    prompt.home();
    prompt.delete();
    assert_eq!(prompt.query(), "a");

    // a ZWJ family emoji is five code points, one grapheme
    let mut prompt = typed("x\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}");
    prompt.move_left();
    assert_eq!(prompt.before_cursor(), "x");
    prompt.end();
    prompt.backspace();
    assert_eq!(prompt.query(), "x");
  }

  #[test]
  fn cursor_stays_on_char_boundary() {
    let mut prompt = typed("日本語");
    prompt.move_left();
    prompt.insert('é');
    prompt.move_left();
    prompt.move_left();
    prompt.backspace();
    prompt.delete();
    assert!(prompt.query.is_char_boundary(prompt.cursor));
    assert_eq!(prompt.query(), "é語");
  }

  #[test]
  fn edits_clear_error_but_movement_does_not() {
    let mut prompt = with_error(typed("abc"));
    prompt.move_left();
    prompt.home();
    prompt.end();
    assert!(prompt.error.is_some());

    prompt.insert('d');
    assert!(prompt.error.is_none());

    prompt.error = Some(Matcher::new("").unwrap_err());
    prompt.backspace();
    assert!(prompt.error.is_none());

    prompt.error = Some(Matcher::new("").unwrap_err());
    prompt.home();
    prompt.delete();
    assert!(prompt.error.is_none());
  }
}
