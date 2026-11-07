#![allow(dead_code)]

use std::ops::Range;

use memchr::memmem;
use regex::{Regex, RegexBuilder};

/// Mode of search operation / how a search query is interpreted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SearchMode {
  /// Search for a literal substring.
  #[default]
  Substring,
  /// Search with a regular expression.
  Regex,
}

impl SearchMode {
  /// Label used by the search input.
  #[must_use]
  pub const fn label(self) -> &'static str {
    match self {
      Self::Substring => "[literal]",
      Self::Regex => "[regex]",
    }
  }
}

/// Case-sensitivity for search.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CaseSensitivity {
  /// Case-insensitive if query has no uppercase characters, case-sensitive otherwise.
  #[default]
  Smart,
  /// Always case-insensitive.
  Insensitive,
  /// Always case-sensitive.
  Sensitive,
}

impl CaseSensitivity {
  /// Determine whether matching should be case-insensitive for `query`.
  #[must_use]
  pub fn is_case_insensitive(self, query: &str) -> bool {
    match self {
      Self::Smart => !query.chars().any(char::is_uppercase),
      Self::Insensitive => true,
      Self::Sensitive => false,
    }
  }
}

/// Direction for search navigation through a document.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SearchDirection {
  /// Search forward (downwards).
  #[default]
  Forward,
  /// Search backward (upwards).
  Backward,
}

#[derive(Clone, Debug)]
enum Kind {
  Substring(Vec<u8>),
  Regex(Regex),
}

impl Kind {
  fn with_values(
    query: &str,
    mode: SearchMode,
    case_sensitivity: CaseSensitivity,
  ) -> Result<Self, SearchError> {
    let is_case_insensitive = case_sensitivity.is_case_insensitive(query);
    let kind = match mode {
      SearchMode::Substring if is_case_insensitive => {
        let regex = RegexBuilder::new(&regex::escape(query))
          .case_insensitive(true)
          .build()?;
        Self::Regex(regex)
      },
      SearchMode::Substring => Self::Substring(query.as_bytes().to_vec()),
      SearchMode::Regex => {
        let regex = RegexBuilder::new(query)
          .case_insensitive(is_case_insensitive)
          .build()?;
        Self::Regex(regex)
      },
    };
    Ok(kind)
  }
}

/// A match in a line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchHit {
  /// Logical buffer line containing the match.
  pub line: usize,
  /// Byte range in the line containing the match.
  pub range: Range<usize>,
}

/// A compiled search query.
#[derive(Clone, Debug)]
pub struct Matcher {
  query: String,
  mode: SearchMode,
  case_sensitivity: CaseSensitivity,
  kind: Kind,
}

impl Matcher {
  /// Compile a `query` with default search mode and case sensitivity.
  ///
  /// # Errors
  ///
  /// Returns a [`SearchError`] for an empty or invalid query.
  pub fn new(query: &str) -> Result<Self, SearchError> {
    if query.is_empty() {
      return Err(SearchError::empty());
    }
    let kind = Kind::with_values(query, SearchMode::default(), CaseSensitivity::default())?;
    Ok(Self {
      query: query.to_string(),
      mode: SearchMode::default(),
      case_sensitivity: CaseSensitivity::default(),
      kind,
    })
  }

  pub fn with_values(
    query: &str,
    mode: SearchMode,
    case_sensitivity: CaseSensitivity,
  ) -> Result<Self, SearchError> {
    let kind = Kind::with_values(query, mode, case_sensitivity)?;
    Ok(Self {
      query: query.to_string(),
      mode,
      case_sensitivity,
      kind,
    })
  }

  /// Re-compile a matcher with given `case_sensitivity`.
  pub fn with_case(&mut self, case_sensitivity: CaseSensitivity) -> Result<(), SearchError> {
    if self.query.is_empty() {
      return Err(SearchError::empty());
    }
    self.kind = Kind::with_values(&self.query, self.mode, case_sensitivity)?;
    self.case_sensitivity = case_sensitivity;
    Ok(())
  }

  /// Re-compile a matcher with the given `mode`.
  pub fn with_mode(&mut self, mode: SearchMode) -> Result<(), SearchError> {
    self.kind = Kind::with_values(&self.query, mode, self.case_sensitivity)?;
    self.mode = mode;
    Ok(())
  }

  /// The original query text.
  #[must_use]
  pub fn query(&self) -> &str {
    &self.query
  }

  /// Mode used to compile the query.
  #[must_use]
  pub const fn mode(&self) -> SearchMode {
    self.mode
  }

  /// Case sensitivity configuration.
  #[must_use]
  pub const fn case_sensitivity(&self) -> CaseSensitivity {
    self.case_sensitivity
  }

  /// Whether this matcher executes case-insensitively.
  #[must_use]
  pub fn is_case_insensitive(&self) -> bool {
    self.case_sensitivity.is_case_insensitive(&self.query)
  }

  /// Find the first match starting at or after byte offset `from`.
  #[must_use]
  pub fn find(&self, text: &str, from: usize) -> Option<Range<usize>> {
    match &self.kind {
      Kind::Substring(needle) => substring_find(text, needle, from),
      Kind::Regex(regex) => regex_find(regex, text, from),
    }
  }

  /// Find the last match ending at or before byte offset `before`.
  #[must_use]
  pub fn rfind(&self, text: &str, before: usize) -> Option<Range<usize>> {
    match &self.kind {
      Kind::Substring(needle) => substring_rfind(text, needle, before),
      Kind::Regex(regex) => regex_rfind(regex, text, before),
    }
  }

  /// Find a match in `text` at or after/before `col` in `direction`.
  #[must_use]
  pub fn search_in(
    &self,
    text: &str,
    col: usize,
    direction: SearchDirection,
  ) -> Option<Range<usize>> {
    match direction {
      SearchDirection::Forward => self.find(text, col),
      SearchDirection::Backward => self.rfind(text, col),
    }
  }

  /// Find all non-overlapping matches in `text`.
  pub fn find_iter<'r, 't>(&'r self, text: &'t str) -> Matches<'r, 't> {
    match &self.kind {
      Kind::Substring(needle) => Matches::Substring {
        iter: Box::new(memmem::find_iter(text.as_bytes(), needle)),
        len: needle.len(),
      },
      Kind::Regex(regex) => Matches::Regex(regex.find_iter(text)),
    }
  }
}

/// An iterator yielding non-overlapping match ranges in a line.
pub enum Matches<'r, 't> {
  Substring {
    iter: Box<memmem::FindIter<'t, 'r>>,
    len: usize,
  },
  Regex(regex::Matches<'r, 't>),
}

impl Iterator for Matches<'_, '_> {
  type Item = Range<usize>;

  fn next(&mut self) -> Option<Self::Item> {
    match self {
      Self::Substring { iter, len } => iter.next().map(|start| start..start + *len),
      Self::Regex(matches) => matches.find(|m| !m.is_empty()).map(|m| m.range()),
    }
  }
}

fn substring_find(text: &str, needle: &[u8], from: usize) -> Option<Range<usize>> {
  if from > text.len() || !text.is_char_boundary(from) {
    return None;
  }
  let haystack = text.as_bytes().get(from..)?;
  let offset = memmem::find(haystack, needle)?;
  let start = from + offset;
  Some(start..start + needle.len())
}

fn substring_rfind(text: &str, needle: &[u8], before: usize) -> Option<Range<usize>> {
  let before = before.min(text.len());
  if !text.is_char_boundary(before) {
    return None;
  }
  let haystack = text.as_bytes().get(..before)?;
  let start = memmem::rfind(haystack, needle)?;
  Some(start..start + needle.len())
}

fn regex_find(regex: &Regex, text: &str, from: usize) -> Option<Range<usize>> {
  if from > text.len() || !text.is_char_boundary(from) {
    return None;
  }
  let haystack = text.get(from..)?;
  regex
    .find_iter(haystack)
    .find(|matched| !matched.is_empty())
    .map(|matched| from + matched.start()..from + matched.end())
}

fn regex_rfind(regex: &Regex, text: &str, before: usize) -> Option<Range<usize>> {
  let before = before.min(text.len());
  if !text.is_char_boundary(before) {
    return None;
  }
  let haystack = text.get(..before)?;
  regex
    .find_iter(haystack)
    .filter(|matched| !matched.is_empty())
    .last()
    .map(|matched| matched.range())
}

/// An error produced while compiling a search query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchError {
  message: String,
}

impl SearchError {
  fn empty() -> Self {
    Self {
      message: "search pattern is empty".to_string(),
    }
  }

  fn invalid_regex(error: &regex::Error) -> Self {
    let regex::Error::Syntax(message) = error else {
      return Self {
        message: error.to_string(),
      };
    };

    let detail = message.lines().last().map_or("syntax error", |line| {
      line.strip_prefix("error: ").unwrap_or(line)
    });

    Self {
      message: format!("invalid regex: {detail}"),
    }
  }

  /// Human-readable error description.
  #[must_use]
  pub fn message(&self) -> &str {
    &self.message
  }
}

impl std::fmt::Display for SearchError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str(&self.message)
  }
}

impl From<regex::Error> for SearchError {
  fn from(error: regex::Error) -> Self {
    Self::invalid_regex(&error)
  }
}

impl std::error::Error for SearchError {}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
  use super::*;

  #[test]
  fn query_validation() {
    assert!(
      Matcher::new("")
        .map(|mut m| m.with_mode(SearchMode::Substring))
        .is_err()
    );
    assert!(Matcher::with_values("", SearchMode::Regex, CaseSensitivity::default()).is_err());
    let err =
      Matcher::with_values("[invalid", SearchMode::Regex, CaseSensitivity::default()).unwrap_err();
    assert!(err.message().contains("invalid regex"));
  }

  #[test]
  fn matching_find_rfind_and_smart_case() {
    let sub =
      Matcher::with_values("test", SearchMode::Substring, CaseSensitivity::default()).unwrap();
    let text = "test 123 test 456 test";
    assert_eq!(sub.find(text, 13), Some(18..22));
    assert_eq!(sub.rfind(text, 18), Some(9..13));
    assert_eq!(
      sub.search_in(text, 13, SearchDirection::Forward),
      Some(18..22)
    );
    assert_eq!(
      sub.search_in(text, 18, SearchDirection::Backward),
      Some(9..13)
    );

    // Smart-case: lowercase query matches case-insensitively
    let smart =
      Matcher::with_values("hello", SearchMode::Substring, CaseSensitivity::default()).unwrap();
    assert!(smart.is_case_insensitive());
    assert_eq!(smart.find("Hello HELLO", 0), Some(0..5));

    // Smart-case: uppercase query matches case-sensitively
    let exact =
      Matcher::with_values("Hello", SearchMode::Substring, CaseSensitivity::default()).unwrap();
    assert!(!exact.is_case_insensitive());
    assert_eq!(exact.find("hello HELLO", 0), None);

    // Regex smart-case
    let re =
      Matcher::with_values(r"err\w+", SearchMode::Regex, CaseSensitivity::default()).unwrap();
    assert_eq!(re.find("ERROR occurred", 0), Some(0..5));
  }

  #[test]
  fn find_iter_matches() {
    let matcher =
      Matcher::with_values("aa", SearchMode::Substring, CaseSensitivity::default()).unwrap();
    let text = "aaaa aa";
    let hits: Vec<Range<usize>> = matcher.find_iter(text).collect();
    assert_eq!(hits, vec![0..2, 2..4, 5..7]);
  }
}
