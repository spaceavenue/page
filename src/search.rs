pub mod settings;

use std::ops::Range;

use memchr::memmem;
use regex::{Regex, RegexBuilder};

use crate::search::settings::{SearchCase, SearchMode, SearchSettings};

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
  fn with_settings(query: &str, settings: SearchSettings) -> Result<Self, SearchError> {
    let is_case_insensitive = settings.case.is_case_insensitive(query);
    let kind = match settings.mode {
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
  settings: SearchSettings,
  kind: Kind,
}

impl Matcher {
  /// Compile a `query` with `settings`.
  ///
  /// # Errors
  ///
  /// Returns a [`SearchError`] for an empty or invalid query.
  pub fn with_settings(query: &str, settings: SearchSettings) -> Result<Self, SearchError> {
    if query.is_empty() {
      return Err(SearchError::empty());
    }
    let kind = Kind::with_settings(query, settings)?;
    Ok(Self {
      query: query.to_string(),
      settings,
      kind,
    })
  }

  /// Re-compile a matcher with given `case_sensitivity`.
  pub fn with_case(&mut self, case: SearchCase) -> Result<(), SearchError> {
    if self.query.is_empty() {
      return Err(SearchError::empty());
    }
    self.kind = Kind::with_settings(&self.query, SearchSettings::with_values(self.mode(), case))?;
    self.settings.case = case;
    Ok(())
  }

  /// Re-compile a matcher with the given `mode`.
  pub fn with_mode(&mut self, mode: SearchMode) -> Result<(), SearchError> {
    self.kind = Kind::with_settings(&self.query, SearchSettings::with_values(mode, self.case()))?;
    self.settings.mode = mode;
    Ok(())
  }

  /// Search mode configuration.
  pub const fn mode(&self) -> SearchMode {
    self.settings.mode
  }

  /// Case sensitivity configuration.
  #[must_use]
  pub const fn case(&self) -> SearchCase {
    self.settings.case
  }

  /// Find the first match starting at or after byte offset `from`.
  #[must_use]
  pub fn find(&self, text: &str, from: usize) -> Option<Range<usize>> {
    self.find_iter(text).find(|r| r.start >= from)
  }

  /// Find the last match ending at or before byte offset `before`.
  #[must_use]
  pub fn rfind(&self, text: &str, before: usize) -> Option<Range<usize>> {
    self.find_iter(text).take_while(|r| r.end <= before).last()
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
        text,
        needle,
        pos: 0,
      },
      Kind::Regex(regex) => Matches::Regex(regex.find_iter(text)),
    }
  }
}

/// An iterator yielding non-overlapping match ranges in a line.
pub enum Matches<'r, 't> {
  Substring {
    text: &'t str,
    needle: &'r [u8],
    pos: usize,
  },
  Regex(regex::Matches<'r, 't>),
}

impl Iterator for Matches<'_, '_> {
  type Item = Range<usize>;

  fn next(&mut self) -> Option<Self::Item> {
    match self {
      Self::Substring { text, needle, pos } => {
        let hit = {
          let from = *pos;
          if from > text.len() || !text.is_char_boundary(from) {
            return None;
          }
          let haystack = text.as_bytes().get(from..)?;
          let offset = memmem::find(haystack, needle)?;
          let start = from + offset;
          Some(start..start + needle.len())
        }?;
        *pos = hit.end;
        Some(hit)
      },
      Self::Regex(matches) => matches.find(|m| !m.is_empty()).map(|m| m.range()),
    }
  }
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
  impl Matcher {
    pub(crate) fn new(query: &str) -> Result<Self, SearchError> {
      Self::with_settings(query, SearchSettings::default())
    }

    fn is_case_insensitive(&self) -> bool {
      self.case().is_case_insensitive(&self.query)
    }
  }

  fn matcher(query: &str, mode: SearchMode) -> Result<Matcher, SearchError> {
    Matcher::with_settings(
      query,
      SearchSettings {
        mode,
        ..SearchSettings::default()
      },
    )
  }

  #[test]
  fn query_validation() {
    assert!(
      Matcher::new("")
        .map(|mut m| m.with_mode(SearchMode::Substring))
        .is_err()
    );
    assert!(matcher("", SearchMode::Regex).is_err());
    let err = matcher("[invalid", SearchMode::Regex).unwrap_err();
    assert!(err.message().contains("invalid regex"));
  }

  #[test]
  fn matching_find_rfind_and_smart_case() {
    let sub = matcher("test", SearchMode::Substring).unwrap();
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
    let smart = matcher("hello", SearchMode::Substring).unwrap();
    assert!(smart.is_case_insensitive());
    assert_eq!(smart.find("Hello HELLO", 0), Some(0..5));

    // Smart-case: uppercase query matches case-sensitively
    let exact = matcher("Hello", SearchMode::Substring).unwrap();
    assert!(!exact.is_case_insensitive());
    assert_eq!(exact.find("hello HELLO", 0), None);

    // Regex smart-case
    let re = matcher(r"err\w+", SearchMode::Regex).unwrap();
    assert_eq!(re.find("ERROR occurred", 0), Some(0..5));
  }

  #[test]
  fn find_iter_matches() {
    let matcher = matcher("aa", SearchMode::Substring).unwrap();
    let text = "aaaa aa";
    let hits: Vec<Range<usize>> = matcher.find_iter(text).collect();
    assert_eq!(hits, vec![0..2, 2..4, 5..7]);
  }

  #[test]
  #[allow(clippy::single_range_in_vec_init)]
  fn navigation_agrees_with_find_iter() {
    use SearchMode::{Regex, Substring};
    // (query, mode, text, matches the highlighter shows)
    let cases: [(&str, SearchMode, &str, &[Range<usize>]); 7] = [
      ("aa", Substring, "aaaa", &[0..2, 2..4]),
      // overlapping candidates: only the left-aligned one exists
      ("aa", Substring, "aaa", &[0..2]),
      // anchors must see the whole line, not the slice after the cursor
      ("^foo", Regex, "foo foo", &[0..3]),
      ("foo$", Regex, "foo foo", &[4..7]),
      // empty matches are never hits
      ("a*", Regex, "baab", &[1..3]),
      ("é", Substring, "éaéé", &[0..2, 3..5, 5..7]),
      ("hello", Substring, "Hello HELLO", &[0..5, 6..11]),
    ];

    for (query, mode, text, expected) in cases {
      let m = matcher(query, mode).unwrap();
      let shown: Vec<_> = m.find_iter(text).collect();
      assert_eq!(shown, expected, "find_iter on {query:?} in {text:?}");

      // stepping with `n` visits exactly the shown matches
      let (mut forward, mut from) = (Vec::new(), 0);
      while let Some(hit) = m.find(text, from) {
        from = hit.end;
        forward.push(hit);
      }
      assert_eq!(forward, expected, "forward on {query:?} in {text:?}");

      // and stepping with `N` visits them in reverse
      let (mut backward, mut before) = (Vec::new(), text.len());
      while let Some(hit) = m.rfind(text, before) {
        before = hit.start;
        backward.push(hit);
      }
      backward.reverse();
      assert_eq!(backward, expected, "backward on {query:?} in {text:?}");
    }
  }
}
