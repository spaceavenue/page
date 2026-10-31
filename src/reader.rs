pub struct Reader<'a> {
  text: &'a str,
}

impl<'a> Reader<'a> {
  #[must_use]
  pub const fn new(text: &'a str) -> Self {
    Self { text }
  }

  #[must_use]
  pub fn peek(&self) -> Option<char> {
    self.text.chars().next()
  }

  pub fn bump(&mut self) -> Option<char> {
    let mut chars = self.text.chars();
    let ch = chars.next()?;
    self.text = chars.as_str();
    Some(ch)
  }

  pub fn read_while(&mut self, mut pred: impl FnMut(char) -> bool) -> &'a str {
    let end = self.text.find(|ch| !pred(ch)).unwrap_or(self.text.len());
    let (head, tail) = self.text.split_at_checked(end).unwrap_or((self.text, ""));
    self.text = tail;
    head
  }
  pub fn eat(&mut self, expected: char) -> bool {
    let hit = self.peek() == Some(expected);
    if hit {
      self.bump();
    }
    hit
  }

  #[must_use]
  pub const fn remaining(&self) -> &'a str {
    self.text
  }
}
