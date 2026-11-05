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
  let style = sgr("1;31;44");
  assert_eq!(style.fg, Some(Color::DarkRed));
  assert_eq!(style.bg, Some(Color::DarkBlue));
  assert!(style.attrs.has(Attribute::Bold));

  assert_eq!(apply_sgr(style, "0"), Style::default());
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
fn sgr_extended_color_and_attribute_formats() {
  // Semicolon-delimited 256 and RGB colors
  assert_eq!(sgr("38;5;196").fg, Some(Color::AnsiValue(196)));
  assert_eq!(sgr("48;2;10;20;30").bg, Some(Color::from((10, 20, 30))));

  // Colon-delimited subparameters
  assert_eq!(sgr("38:5:196").fg, Some(Color::AnsiValue(196)));
  assert_eq!(sgr("38:2:10:20:30").fg, Some(Color::from((10, 20, 30))));
  assert_eq!(sgr("48:2::10:20:30").bg, Some(Color::from((10, 20, 30))));

  // Mixed parameters and attributes
  let mixed = sgr("1;38:5:196;4:3;48;2;10;20;30");
  assert!(mixed.attrs.has(Attribute::Bold));
  assert!(mixed.attrs.has(Attribute::Underlined));
  assert_eq!(mixed.fg, Some(Color::AnsiValue(196)));
  assert_eq!(mixed.bg, Some(Color::from((10, 20, 30))));
}

#[test]
fn underline_variants_and_disable() {
  assert!(sgr("4:3").attrs.has(Attribute::Underlined));
  assert!(!sgr("4:0").attrs.has(Attribute::Underlined));
}

#[test]
fn invalid_and_boundary_sgr_parameters_ignored() {
  for invalid in [
    "not-a-number",
    "100000",
    "38;5;99999999999",
    "38;5;300",
    "38;2;1000;20;30",
    "38:2:1000:20:30",
    "38;5",
    "38:5",
    "38;2;10",
    "38:2:10",
  ] {
    assert_eq!(
      apply_sgr(Style::default(), invalid),
      Style::default(),
      "failed on {invalid}"
    );
  }
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

impl ParsedLine {
  fn style_at(&self, byte: usize) -> Style {
    let first = self
      .runs
      .partition_point(|run| run.start <= byte)
      .saturating_sub(1);
    self
      .runs
      .get(first)
      .map_or_else(Style::default, |run| run.style)
  }

  fn column_at(&self, offset: usize) -> usize {
    let offset = offset.min(self.text.len());
    let mut boundary = 0usize;

    for (index, grapheme) in self.text.grapheme_indices(true) {
      if index > offset {
        break;
      }
      boundary = index;
      if index + grapheme.len() > offset {
        break;
      }
      boundary = index + grapheme.len();
    }

    UnicodeWidthStr::width(self.text.get(..boundary).unwrap_or_default())
  }
}

fn parsed(raw: &[u8]) -> ParsedLine {
  let mut line = ParsedLine::default();
  line.parse(raw, true, TAB_WIDTH);
  line
}

fn rows(line: &ParsedLine, width: usize, word_wrap: bool) -> Vec<&str> {
  let mode = if word_wrap {
    WrapMode::Soft
  } else {
    WrapMode::Hard
  };
  line
    .rows(width, mode)
    .map(|range| line.text.get(range).unwrap_or_default())
    .collect()
}

#[test]
fn plain_text_unchanged() {
  let line = parsed(b"hello");
  assert_eq!(line.text, "hello");
  assert_eq!(line.count_rows(80, WrapMode::Soft), 1);
  assert_eq!(rows(&line, 80, true), vec!["hello"]);
}

#[test]
fn tabs_expand_to_next_stop() {
  let line = parsed(b"a\tb");
  assert_eq!(line.text, "a       b");
}

#[test]
fn trailing_cr_stripped() {
  let line = parsed(b"hello\r\n");
  assert_eq!(line.text, "hello");
}

#[test]
fn control_bytes_use_caret_notation() {
  let line = parsed(b"a\x01b\x7f");
  assert_eq!(line.text, "a^Ab^?");
}

#[test]
fn sgr_styles_applied_and_text_is_clean() {
  let line = parsed(b"\x1b[31mred\x1b[0m plain");
  assert_eq!(line.text, "red plain");
  assert_eq!(line.style_at(0).fg, Some(Color::DarkRed));
  assert_eq!(line.style_at(3), Style::default());
}

#[test]
fn non_sgr_escapes_removed() {
  let line = parsed(b"a\x1b[2Jb\x1b]0;title\x07c");
  assert_eq!(line.text, "abc");
}

#[test]
fn c1_controls_never_reach_terminal() {
  let line = parsed("\u{009b}31m".as_bytes());
  assert!(!line.text.contains('\u{009b}'));
}

#[test]
fn hard_wrap_breaks_at_column_limit() {
  let line = parsed(b"abcdefgh");
  assert_eq!(rows(&line, 3, false), vec!["abc", "def", "gh"]);
  assert_eq!(rows(&line, 3, true), vec!["abc", "def", "gh"]);
  assert_eq!(line.count_rows(3, WrapMode::Hard), 3);
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
  assert_eq!(line.count_rows(80, WrapMode::Soft), 1);
  assert_eq!(line.rows(80, WrapMode::Soft).next(), Some(0..0));
}

#[test]
fn count_rows_matches_full_wrapping() {
  let line = parsed(b"aaaa bbbb cccc dddd");
  assert_eq!(rows(&line, 5, true).len(), 4);
  assert_eq!(line.count_rows(5, WrapMode::Soft), 4);
}

#[test]
fn render_row_emits_text_and_resets_style() {
  let line = parsed(b"\x1b[31mred\x1b[0m");
  let mut out = Vec::new();
  let range = line.rows(80, WrapMode::Soft).next().unwrap_or(0..0);
  assert!(line.render_range(&mut out, range).is_ok());
  let output = String::from_utf8_lossy(&out);
  assert!(output.contains("red"));
  assert!(output.contains("\x1b[0m"));
}

#[test]
fn custom_tab_width_expands_correctly() {
  let mut line = ParsedLine::default();
  let tab_width = NonZeroUsize::new(4).unwrap_or(NonZeroUsize::MIN);
  line.parse(b"a\tb", true, tab_width);
  assert_eq!(line.text, "a   b");
}

#[test]
fn display_width_uses_terminal_columns() {
  let line = parsed("a中b".as_bytes());
  assert_eq!(line.display_width(), 4);
}

#[test]
fn column_window_slices_ascii() {
  let line = parsed(b"abcdef");
  assert_eq!(
    line.column_window(0, 3),
    ColumnWindow {
      pad: 0,
      range: 0..3
    }
  );
  assert_eq!(
    line.column_window(2, 3),
    ColumnWindow {
      pad: 0,
      range: 2..5
    }
  );
  assert_eq!(
    line.column_window(10, 3),
    ColumnWindow {
      pad: 0,
      range: 6..6
    }
  );
}

#[test]
fn column_window_pads_straddled_wide_characters() {
  let line = parsed("a中b".as_bytes());
  assert_eq!(
    line.column_window(0, 3),
    ColumnWindow {
      pad: 0,
      range: 0..4
    }
  );
  assert_eq!(
    line.column_window(1, 2),
    ColumnWindow {
      pad: 0,
      range: 1..4
    }
  );
  assert_eq!(
    line.column_window(2, 2),
    ColumnWindow {
      pad: 1,
      range: 4..5
    }
  );
}

#[test]
fn column_window_keeps_zero_width_characters_with_base() {
  let line = parsed("a\u{0301}b".as_bytes());
  assert_eq!(
    line.column_window(0, 1),
    ColumnWindow {
      pad: 0,
      range: 0.."a\u{0301}".len()
    }
  );
}

fn bold(text: &str) -> String {
  text.chars().flat_map(|ch| [ch, '\x08', ch]).collect()
}

fn underline(text: &str) -> String {
  text.chars().flat_map(|ch| ['_', '\x08', ch]).collect()
}

#[test]
fn overstrike_bold_and_underline() {
  let raw = format!("{} {}", bold("NAME"), underline("<option>"));
  let line = parsed(raw.as_bytes());
  assert_eq!(line.text, "NAME <option>");
  assert!(line.style_at(0).attrs.has(Attribute::Bold));
  assert!(!line.style_at(4).attrs.has(Attribute::Bold));
  assert!(line.style_at(5).attrs.has(Attribute::Underlined));
}

#[test]
fn overstrike_combines_with_sgr() {
  let raw = format!("\x1b[31m{}\x1b[0m plain", bold("red"));
  let line = parsed(raw.as_bytes());
  assert_eq!(line.text, "red plain");
  let style = line.style_at(0);
  assert_eq!(style.fg, Some(Color::DarkRed));
  assert!(style.attrs.has(Attribute::Bold));
  assert_eq!(line.style_at(3), Style::default());
}

#[test]
fn stray_backspace_stays_caret_notation() {
  let line = parsed(b"a\x08b");
  assert_eq!(line.text, "a^Hb");
}

#[test]
fn zwj_emoji_one_grapheme() {
  let family = "👨‍👩‍👧‍👦";
  let line = parsed(family.as_bytes());
  assert_eq!(line.display_width(), 2);
  assert_eq!(rows(&line, 3, false), vec![family]);
}

#[test]
fn zwj_emoji_not_split_by_wrap() {
  let family = "👨‍👩‍👧‍👦";
  let line = parsed(format!("{family}ab").as_bytes());
  let first = format!("{family}a");
  assert_eq!(rows(&line, 3, false), vec![first.as_str(), "b"]);
}

#[test]
fn flag_is_two_columns() {
  let flag = "🇺🇸";
  let line = parsed(flag.as_bytes());
  assert_eq!(line.display_width(), 2);
  assert_eq!(rows(&line, 2, false), vec![flag]);
}

#[test]
fn column_at_floors_to_grapheme_boundaries() {
  let family = "👨‍👩‍👧‍👦";
  let line = parsed(format!("{family}ab").as_bytes());
  assert_eq!(line.column_at(0), 0);
  assert_eq!(line.column_at(1), 0);
  assert_eq!(line.column_at(family.len()), 2);
  assert_eq!(line.column_at(line.text.len()), 4);
}

#[test]
fn tab_after_wide_grapheme_aligns() {
  let family = "👨‍👩‍👧‍👦";
  let line = parsed(format!("{family}\tb").as_bytes());
  let spaces = " ".repeat(6);
  let expected = format!("{family}{spaces}b");
  assert_eq!(line.text, expected);
}
