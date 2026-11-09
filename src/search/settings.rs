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
  /// The other mode.
  #[must_use]
  pub const fn toggled(self) -> Self {
    match self {
      Self::Substring => Self::Regex,
      Self::Regex => Self::Substring,
    }
  }

  /// Label used by the search input.
  #[must_use]
  pub const fn label(self) -> &'static str {
    match self {
      Self::Substring => "[substring]",
      Self::Regex => "[regex]",
    }
  }
}

/// Case-sensitivity for search.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SearchCase {
  /// Case-insensitive if query has no uppercase characters, case-sensitive otherwise.
  #[default]
  Smart,
  /// Always case-insensitive.
  Insensitive,
  /// Always case-sensitive.
  Sensitive,
}

impl SearchCase {
  /// Label used by the search input.
  pub const fn label(self) -> &'static str {
    match self {
      Self::Smart => "[smart-case]",
      Self::Insensitive => "[ignore-case]",
      Self::Sensitive => "[match-case]",
    }
  }

  /// Cycle to the next case sensitivity.
  pub const fn next(self) -> Self {
    match self {
      Self::Smart => Self::Sensitive,
      Self::Insensitive => Self::Smart,
      Self::Sensitive => Self::Insensitive,
    }
  }

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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SearchSettings {
  pub mode: SearchMode,
  pub case: SearchCase,
}

impl SearchSettings {
  pub const fn with_values(mode: SearchMode, case: SearchCase) -> Self {
    Self { mode, case }
  }
}
