use std::ops::{Deref, DerefMut};

use crossterm::style::{Attribute, Color};

#[cfg(test)]
use super::*;
use crate::text::ansi::{apply_sgr, escape};

fn sgr(params: &str) -> Style {
  apply_sgr(Style::default(), params)
}

pub fn escape_slice(input: &str, enabled: bool, style: Style) -> (usize, Style) {
  let mut reader = Reader::new(input);
  let style = escape(&mut reader, enabled, style);
  let consumed = input.len() - reader.remaining().len();
  (consumed, style)
}

#[test]
fn reset_clears_everything() {
  let mut style = sgr("1;31;44");
  assert_eq!(style.fg, Some(Color::DarkRed));
  assert_eq!(style.bg, Some(Color::DarkBlue));
  assert!(style.attrs.has(Attribute::Bold));

  style = apply_sgr(style, "0");
  assert_eq!(style, Style::default());
}

#[test]
fn empty_parameters_reset() {
  assert_eq!(sgr(""), Style::default());
}

#[test]
fn attributes_turned_off_individually() {
  let style = sgr("1;2;3;4;7;9");
  assert!(style.attrs.has(Attribute::Bold));
  assert!(style.attrs.has(Attribute::Dim));

  let style = apply_sgr(style, "22");
  assert!(!style.attrs.has(Attribute::Bold));
  assert!(!style.attrs.has(Attribute::Dim));
  assert!(style.attrs.has(Attribute::Italic));
}

#[test]
fn bright_and_basic_colors_distinct() {
  assert_eq!(sgr("31").fg, Some(Color::DarkRed));
  assert_eq!(sgr("91").fg, Some(Color::Red));
  assert_eq!(sgr("44").bg, Some(Color::DarkBlue));
  assert_eq!(sgr("104").bg, Some(Color::Blue));
}

#[test]
fn indexed_and_rgb_colors() {
  assert_eq!(sgr("38;5;196").fg, Some(Color::AnsiValue(196)));
  assert_eq!(sgr("48;2;10;20;30").bg, Some(Color::from((10, 20, 30))));
}

#[test]
fn colon_subparameters_supported() {
  assert_eq!(sgr("38:5:196").fg, Some(Color::AnsiValue(196)));
  assert_eq!(sgr("38:2:10:20:30").fg, Some(Color::from((10, 20, 30))));
  assert_eq!(sgr("48:2::10:20:30").bg, Some(Color::from((10, 20, 30))));
}

#[test]
fn underline_variants_and_disable() {
  assert!(sgr("4:3").attrs.has(Attribute::Underlined));
  assert!(!sgr("4:0").attrs.has(Attribute::Underlined));
}

#[test]
fn invalid_parameters_ignored() {
  let style = Style {
    fg: Some(Color::Red),
    bg: None,
    attrs: Attribute::Bold.into(),
  };
  assert_eq!(apply_sgr(style, "not-a-number"), style);
}

#[test]
fn default_colors_can_be_restored() {
  let mut style = sgr("31;44");
  style = apply_sgr(style, "39;49");
  assert_eq!(style.fg, None);
  assert_eq!(style.bg, None);
}

#[test]
fn non_sgr_sequences_do_not_change_style() {
  let original = Style {
    fg: Some(Color::Red),
    bg: None,
    attrs: Attribute::Bold.into(),
  };
  let (consumed, style) = escape_slice("\x1b[2J", true, original);
  assert_eq!(consumed, 4);
  assert_eq!(style, original);
}

#[test]
fn osc_skipped_to_bel_or_st() {
  let (consumed, _) = escape_slice("\x1b]0;title\x07rest", true, Style::default());
  assert_eq!(consumed, 10);

  let (consumed, _) = escape_slice("\x1b]8;;https://example.com\x1b\\x", true, Style::default());
  assert_eq!(consumed, 26);
}

#[test]
fn truncated_csi_is_consumed_without_panicking() {
  let (consumed, style) = escape_slice("\x1b[31", true, Style::default());
  assert_eq!(consumed, 4);
  assert_eq!(style, Style::default());
}

#[test]
fn escape_advances_reader() {
  let mut reader = Reader::new("\x1b[31mhello");
  let style = escape(&mut reader, true, Style::default());
  assert_eq!(style.fg, Some(Color::DarkRed));
  assert_eq!(reader.remaining(), "hello");
}

#[test]
fn overflow_numbers_do_not_panic() {
  assert_eq!(apply_sgr(Style::default(), "100000"), Style::default());
  assert_eq!(
    apply_sgr(Style::default(), "38;5;99999999999"),
    Style::default()
  );
}

#[test]
fn out_of_range_colors_are_ignored() {
  assert_eq!(apply_sgr(Style::default(), "38;5;300"), Style::default());
  assert_eq!(
    apply_sgr(Style::default(), "38;2;1000;20;30"),
    Style::default()
  );
  assert_eq!(
    apply_sgr(Style::default(), "38:2:1000:20:30"),
    Style::default()
  );
}

#[test]
fn mixed_semicolon_and_colon_parameters() {
  let style = sgr("1;38:5:196;4:3;48;2;10;20;30");
  assert!(style.attrs.has(Attribute::Bold));
  assert!(style.attrs.has(Attribute::Underlined));
  assert_eq!(style.fg, Some(Color::AnsiValue(196)));
  assert_eq!(style.bg, Some(Color::from((10, 20, 30))));
}

#[test]
fn truncated_extended_colors_dont_panic_or_apply() {
  assert_eq!(apply_sgr(Style::default(), "38;5"), Style::default());
  assert_eq!(apply_sgr(Style::default(), "38;2;10"), Style::default());
  assert_eq!(apply_sgr(Style::default(), "38:5"), Style::default());
  assert_eq!(apply_sgr(Style::default(), "38:2:10"), Style::default());
}

#[test]
fn unexpected_subparameters_skipped() {
  // `1:2;31` has a stray subparameter on bold; bold should apply and 31 should be red
  let style = sgr("1:2;31");
  assert!(style.attrs.has(Attribute::Bold));
  assert_eq!(style.fg, Some(Color::DarkRed));

  // Unknown color subtype with subparams should skip them and keep parsing
  let style = sgr("38:9:1:2;31");
  assert_eq!(style.fg, Some(Color::DarkRed));
}

const TAB_WIDTH: NonZeroUsize = match NonZeroUsize::new(8) {
  Some(n) => n,
  None => unreachable!(),
};

#[derive(Debug, Default)]
struct TestLine(ParsedLine);

impl Deref for TestLine {
  type Target = ParsedLine;

  fn deref(&self) -> &Self::Target {
    &self.0
  }
}

impl DerefMut for TestLine {
  fn deref_mut(&mut self) -> &mut Self::Target {
    &mut self.0
  }
}

impl TestLine {
  fn text(&self) -> &str {
    self.0.text()
  }

  fn style_at(&self, byte: usize) -> Style {
    let first = self
      .0
      .runs()
      .partition_point(|run| run.start() <= byte)
      .saturating_sub(1);
    self
      .0
      .runs()
      .get(first)
      .map_or_else(Style::default, |run| run.style())
  }
}

fn parsed(raw: &[u8]) -> TestLine {
  let mut line = ParsedLine::default();
  line.parse(raw, true, TAB_WIDTH);
  TestLine(line)
}

fn rows(line: &TestLine, width: usize, word_wrap: bool) -> Vec<&str> {
  line
    .rows(width, word_wrap)
    .map(|range| line.text().get(range).unwrap_or_default())
    .collect()
}

#[test]
fn plain_text_unchanged() {
  let line = parsed(b"hello");
  assert_eq!(line.text(), "hello");
  assert_eq!(line.count_rows(80, true), 1);
  assert_eq!(rows(&line, 80, true), vec!["hello"]);
}

#[test]
fn tabs_expand_to_next_stop() {
  let line = parsed(b"a\tb");
  assert_eq!(line.text(), "a       b");
}

#[test]
fn trailing_cr_stripped() {
  let line = parsed(b"hello\r\n");
  assert_eq!(line.text(), "hello");
}

#[test]
fn control_bytes_use_caret_notation() {
  let line = parsed(b"a\x01b\x7f");
  assert_eq!(line.text(), "a^Ab^?");
}

#[test]
fn sgr_styles_applied_and_text_is_clean() {
  let line = parsed(b"\x1b[31mred\x1b[0m plain");
  assert_eq!(line.text(), "red plain");
  assert_eq!(line.style_at(0).fg, Some(Color::DarkRed));
  assert_eq!(line.style_at(3), Style::default());
}

#[test]
fn non_sgr_escapes_removed() {
  let line = parsed(b"a\x1b[2Jb\x1b]0;title\x07c");
  assert_eq!(line.text(), "abc");
}

#[test]
fn c1_controls_never_reach_terminal() {
  let line = parsed("\u{009b}31m".as_bytes());
  assert!(!line.text().contains('\u{009b}'));
}

#[test]
fn hard_wrap_breaks_at_column_limit() {
  let line = parsed(b"abcdefgh");
  assert_eq!(rows(&line, 3, false), vec!["abc", "def", "gh"]);
  assert_eq!(line.count_rows(3, false), 3);
}

#[test]
fn word_wrap_breaks_at_last_space() {
  let line = parsed(b"abc def");
  assert_eq!(rows(&line, 4, true), vec!["abc ", "def"]);
}

#[test]
fn word_wrap_skips_spaces_at_start_of_row() {
  let line = parsed(b"abc   def");
  assert_eq!(rows(&line, 5, true), vec!["abc  ", "def"]);
}

#[test]
fn long_words_hard_broken() {
  let line = parsed(b"abcdefgh");
  assert_eq!(rows(&line, 3, true), vec!["abc", "def", "gh"]);
}

#[test]
fn long_words_after_indentation_use_remaining_columns() {
  let line = parsed(b"  abcdefgh");
  assert_eq!(rows(&line, 5, true), vec!["  abc", "defgh"]);
}

#[test]
fn wide_characters_use_two_columns() {
  let line = parsed("中中中".as_bytes());
  assert_eq!(rows(&line, 4, true), vec!["中中", "中"]);
}

#[test]
fn zero_width_characters_stay_with_base() {
  let line = parsed("a\u{0301}b".as_bytes());
  assert_eq!(rows(&line, 1, true), vec!["a\u{0301}", "b"]);
}

#[test]
fn empty_lines_occupy_one_row() {
  let line = parsed(b"");
  assert_eq!(line.count_rows(80, true), 1);
  assert_eq!(line.rows(80, true).next(), Some(0..0));
}

#[test]
fn limited_wrap_stops_after_requested_rows() {
  let line = parsed(b"aaaa bbbb cccc dddd");
  let rows: Vec<_> = line.rows(5, true).take(2).collect();
  assert_eq!(rows.len(), 2);
}

#[test]
fn count_rows_matches_full_wrapping() {
  let line = parsed(b"aaaa bbbb cccc dddd");
  assert_eq!(rows(&line, 5, true).len(), 4);
  assert_eq!(line.count_rows(5, true), 4);
}

#[test]
fn render_row_emits_text_and_resets_style() {
  let line = parsed(b"\x1b[31mred\x1b[0m");
  let mut out = Vec::new();
  let range = line.rows(80, true).next().unwrap_or(0..0);
  line.render_range(&mut out, range).unwrap();
  let output = String::from_utf8_lossy(&out);
  assert!(output.contains("red"));
  assert!(output.contains("\x1b[0m"));
}

#[test]
fn custom_tab_width_expands_correctly() {
  let mut line = TestLine::default();
  let tab_width = match NonZeroUsize::new(4) {
    Some(width) => width,
    None => unreachable!(),
  };
  line.parse(b"a\tb", true, tab_width);
  assert_eq!(line.text(), "a   b");
}

#[test]
fn display_width_uses_terminal_columns() {
  let line = parsed("a中b".as_bytes());
  assert_eq!(line.display_width(), 4);
}

#[test]
fn column_range_slices_ascii() {
  let line = parsed(b"abcdef");
  assert_eq!(line.column_range(0, 3), 0..3);
  assert_eq!(line.column_range(2, 3), 2..5);
  assert_eq!(line.column_range(10, 3), 6..6);
}

#[test]
fn column_range_respects_wide_characters() {
  let line = parsed("a中b".as_bytes());
  assert_eq!(line.column_range(0, 3), 0..4);
  assert_eq!(line.column_range(1, 2), 1..4);
  assert_eq!(line.column_range(2, 2), 4..5);
}

#[test]
fn column_range_keeps_zero_width_characters_with_their_base() {
  let line = parsed("a\u{0301}b".as_bytes());
  assert_eq!(line.column_range(0, 1), 0.."a\u{0301}".len());
}
