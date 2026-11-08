use unicode_segmentation::UnicodeSegmentation;

use crate::search::SearchError;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchPrompt {
  query: String,
  cursor: usize,
  pub error: Option<SearchError>,
}

impl SearchPrompt {
  pub fn query(&self) -> &str {
    &self.query
  }

  pub fn before_cursor(&self) -> &str {
    self.query.get(..self.cursor).unwrap_or_default()
  }

  pub fn insert(&mut self, ch: char) {
    self.query.insert(self.cursor, ch);
    self.cursor += ch.len_utf8();
    self.error = None;
  }

  pub fn backspace(&mut self) {
    let start = self.prev_boundary();
    self.query.drain(start..self.cursor);
    self.cursor = start;
    self.error = None;
  }

  pub fn delete(&mut self) {
    let end = self.next_boundary();
    self.query.drain(self.cursor..end);
    self.error = None;
  }

  pub fn move_left(&mut self) {
    self.cursor = self.prev_boundary();
  }

  pub fn move_right(&mut self) {
    self.cursor = self.next_boundary();
  }

  pub const fn home(&mut self) {
    self.cursor = 0;
  }

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
  fn insert_goes_at_the_cursor() {
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
  fn cursor_stays_on_a_char_boundary() {
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
  fn edits_clear_the_error_but_movement_does_not() {
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
