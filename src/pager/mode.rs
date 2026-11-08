use crate::pager::prompt::SearchPrompt;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Mode {
  #[default]
  Normal,
  Help,
  #[allow(dead_code)]
  Search(SearchPrompt),
}
