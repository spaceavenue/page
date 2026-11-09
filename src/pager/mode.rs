use crate::pager::prompt::SearchPrompt;

/// Modes held by the pager.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Mode {
  #[default]
  Normal,
  Help,
  Search(SearchPrompt),
}
