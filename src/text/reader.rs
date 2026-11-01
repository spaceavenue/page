/// Forward only reader for an str.
pub struct Reader<'a> {
  text: &'a str,
}

impl<'a> Reader<'a> {
  #[must_use]
  /// Create a new reader from the str.
  pub const fn new(text: &'a str) -> Self {
    Self { text }
  }

  /// Get the current character.
  #[must_use]
  pub(crate) fn peek(&self) -> Option<char> {
    self.text.chars().next()
  }

  /// Get the current character and advance the reader by one.
  pub(crate) fn bump(&mut self) -> Option<char> {
    let mut chars = self.text.chars();
    let ch = chars.next()?;
    self.text = chars.as_str();
    Some(ch)
  }

  /// Read while `pred` is false, then return what was read.
  pub(crate) fn read_while(&mut self, mut pred: impl FnMut(char) -> bool) -> &'a str {
    let end = self.text.find(|ch| !pred(ch)).unwrap_or(self.text.len());
    let (head, tail) = self.text.split_at_checked(end).unwrap_or((self.text, ""));
    self.text = tail;
    head
  }

  /// Read until the delimeter is found.
  pub(crate) fn read_until(&mut self, delimiter: char) -> &'a str {
    self.read_while(|ch| ch != delimiter)
  }

  /// If current char is `expected`, eat it (advance without returning it).
  pub(crate) fn eat(&mut self, expected: char) -> bool {
    let hit = self.peek() == Some(expected);
    if hit {
      self.bump();
    }
    hit
  }

  /// Get the remaining str.
  #[must_use]
  pub const fn remaining(&self) -> &'a str {
    self.text
  }
}
