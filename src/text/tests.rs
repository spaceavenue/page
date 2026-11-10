#![allow(clippy::unwrap_used)]

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
fn sgr_resets() {
  let style = sgr("1;31;44");
  assert_eq!(style.fg, Some(Color::DarkRed));
  assert_eq!(style.bg, Some(Color::DarkBlue));
  assert!(style.attrs.has(Attribute::Bold));

  // full reset, with and without the explicit `0`
  assert_eq!(apply_sgr(style, "0"), Style::default());
  assert_eq!(sgr(""), Style::default());

  // attributes switch off individually
  let style = sgr("1;2;3;4;7;9");
  assert!(style.attrs.has(Attribute::Bold));
  assert!(style.attrs.has(Attribute::Dim));
  let style = apply_sgr(style, "22");
  assert!(!style.attrs.has(Attribute::Bold));
  assert!(!style.attrs.has(Attribute::Dim));
  assert!(style.attrs.has(Attribute::Italic));

  // underline is switched off by the `4:0` subparameter
  assert!(!sgr("4:0").attrs.has(Attribute::Underlined));

  // default colors can be restored
  let style = apply_sgr(sgr("31;44"), "39;49");
  assert_eq!(style.fg, None);
  assert_eq!(style.bg, None);
}

#[test]
fn sgr_color_formats() {
  // basic and bright colors are distinct
  assert_eq!(sgr("31").fg, Some(Color::DarkRed));
  assert_eq!(sgr("91").fg, Some(Color::Red));
  assert_eq!(sgr("44").bg, Some(Color::DarkBlue));
  assert_eq!(sgr("104").bg, Some(Color::Blue));

  // Semicolon-delimited 256 and RGB colors
  assert_eq!(sgr("38;5;196").fg, Some(Color::AnsiValue(196)));
  assert_eq!(sgr("48;2;10;20;30").bg, Some(Color::from((10, 20, 30))));

  // Colon-delimited subparameters
  assert_eq!(sgr("38:5:196").fg, Some(Color::AnsiValue(196)));
  assert_eq!(sgr("38:2:10:20:30").fg, Some(Color::from((10, 20, 30))));
  assert_eq!(sgr("48:2::10:20:30").bg, Some(Color::from((10, 20, 30))));
  assert!(sgr("4:3").attrs.has(Attribute::Underlined));

  // Mixed parameters and attributes
  let mixed = sgr("1;38:5:196;4:3;48;2;10;20;30");
  assert!(mixed.attrs.has(Attribute::Bold));
  assert!(mixed.attrs.has(Attribute::Underlined));
  assert_eq!(mixed.fg, Some(Color::AnsiValue(196)));
  assert_eq!(mixed.bg, Some(Color::from((10, 20, 30))));
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
fn unexpected_subparameters_skipped() {
  // `1:2;31` has a stray subparameter on bold; bold should apply and 31 should be red
  let style = sgr("1:2;31");
  assert!(style.attrs.has(Attribute::Bold));
  assert_eq!(style.fg, Some(Color::DarkRed));

  // Unknown color subtype with subparams should skip them and keep parsing
  let style = sgr("38:9:1:2;31");
  assert_eq!(style.fg, Some(Color::DarkRed));
}

#[test]
fn escape_consumes_whole_sequence() {
  for (input, consumed) in [
    ("\x1b[2J", 4),
    ("\x1b]0;title\x07rest", 10),
    ("\x1b]8;;https://example.com\x1b\\x", 26),
    // truncated CSI is consumed without panicking
    ("\x1b[31", 4),
    ("\x1b[31mhello", 5),
  ] {
    let (got, _) = escape_slice(input, true, Style::default());
    assert_eq!(got, consumed, "failed on {input:?}");
  }

  // a non-SGR sequence leaves the style alone
  let original = Style {
    fg: Some(Color::Red),
    bg: None,
    attrs: Attribute::Bold.into(),
  };
  assert_eq!(escape_slice("\x1b[2J", true, original).1, original);
  assert_eq!(
    escape_slice("\x1b[31", true, Style::default()).1,
    Style::default()
  );
  assert_eq!(
    escape_slice("\x1b[31mhello", true, Style::default()).1.fg,
    Some(Color::DarkRed)
  );
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
fn parse_normalises_text() {
  for (raw, expected) in [
    ("hello", "hello"),
    ("a\tb", "a       b"),
    ("hello\r\n", "hello"),
    // control bytes use caret notation, including a stray backspace
    ("a\x01b\x7f", "a^Ab^?"),
    ("a\x08b", "a^Hb"),
    ("a\x1b[2Jb\x1b]0;title\x07c", "abc"),
  ] {
    assert_eq!(parsed(raw.as_bytes()).text, expected, "failed on {raw:?}");
  }

  let mut line = ParsedLine::default();
  let tab_width = NonZeroUsize::new(4).unwrap_or(NonZeroUsize::MIN);
  line.parse(b"a\tb", true, tab_width);
  assert_eq!(line.text, "a   b");
}

#[test]
fn c1_controls_never_reach_terminal() {
  let line = parsed("\u{009b}31m".as_bytes());
  assert!(!line.text.contains('\u{009b}'));
}

#[test]
fn sgr_styles_applied_and_text_is_clean() {
  let line = parsed(b"\x1b[31mred\x1b[0m plain");
  assert_eq!(line.text, "red plain");
  assert_eq!(line.style_at(0).fg, Some(Color::DarkRed));
  assert_eq!(line.style_at(3), Style::default());
}

#[test]
fn ascii_wrapping() {
  for (text, width, soft, expected) in [
    // hard wrap breaks at the column limit; soft wrap agrees when there is no space
    ("abcdefgh", 3, false, vec!["abc", "def", "gh"]),
    ("abcdefgh", 3, true, vec!["abc", "def", "gh"]),
    ("abc def", 4, true, vec!["abc ", "def"]),
    // spaces at the break are skipped, not carried to the next row
    ("abc   def", 5, true, vec!["abc  ", "def"]),
    // a long word after indentation uses the remaining columns
    ("  abcdefgh", 5, true, vec!["  abc", "defgh"]),
  ] {
    let line = parsed(text.as_bytes());
    assert_eq!(
      rows(&line, width, soft),
      expected,
      "failed on {text:?} width {width} soft {soft}"
    );
  }
  assert_eq!(parsed(b"abcdefgh").count_rows(3, WrapMode::Hard), 3);
}

#[test]
fn unicode_widths_and_wrapping() {
  let family = "👨‍👩‍👧‍👦";
  let flag = "🇺🇸";
  let family_ab = format!("{family}ab");
  let family_a = format!("{family}a");

  // wide characters take two columns, combiners stay with their base, and ZWJ sequences and flags
  // are never split
  for (text, width, soft, expected) in [
    ("中中中", 4, true, vec!["中中", "中"]),
    ("a\u{0301}b", 1, true, vec!["a\u{0301}", "b"]),
    (family, 3, false, vec![family]),
    (flag, 2, false, vec![flag]),
    (family_ab.as_str(), 3, false, vec![family_a.as_str(), "b"]),
  ] {
    let line = parsed(text.as_bytes());
    assert_eq!(
      rows(&line, width, soft),
      expected,
      "failed on {text:?} width {width}"
    );
  }

  for (text, width) in [("a中b", 4), (family, 2), (flag, 2)] {
    assert_eq!(
      parsed(text.as_bytes()).display_width(),
      width,
      "failed on {text:?}"
    );
  }
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
  assert!(line.render_range(&mut out, range, &[]).is_ok());
  let output = String::from_utf8_lossy(&out);
  assert!(output.contains("red"));
  assert!(output.contains("\x1b[0m"));
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
fn tab_after_wide_grapheme_aligns() {
  let family = "👨‍👩‍👧‍👦";
  let line = parsed(format!("{family}\tb").as_bytes());
  let spaces = " ".repeat(6);
  let expected = format!("{family}{spaces}b");
  assert_eq!(line.text, expected);
}

fn ansi(command: impl crossterm::Command) -> String {
  let mut out = Vec::new();
  crossterm::queue!(out, command).unwrap();
  String::from_utf8(out).unwrap()
}

#[test]
fn render_range_applies_highlights() {
  type Case<'a> = (&'a [u8], Vec<Highlight>, std::ops::Range<usize>, String);
  use crossterm::style::{SetAttribute, SetBackgroundColor, SetForegroundColor};

  let reset = ansi(SetAttribute(Attribute::Reset));
  let red = ansi(SetForegroundColor(Color::DarkRed));
  let bg = ansi(SetBackgroundColor(Color::Yellow));
  let hl = |range| Highlight {
    range,
    style: Style {
      bg: Some(Color::Yellow),
      ..Style::default()
    },
  };

  // (raw input, highlights, row range, expected output)
  let cases: Vec<Case> = vec![
    // no highlights: unchanged output
    (b"abcdef", vec![], 0..6, "abcdef".to_string()),
    // inside one run: the text before and after keeps the run style
    (
      b"abcdef",
      vec![hl(2..4)],
      0..6,
      format!("ab{reset}{bg}cd{reset}ef"),
    ),
    // spanning two runs: the highlight keeps the foreground of each run
    (
      b"\x1b[31mab\x1b[0mcd",
      vec![hl(1..3)],
      0..4,
      format!("{reset}{red}a{reset}{red}{bg}b{reset}{bg}c{reset}d"),
    ),
    // ending exactly on a run boundary must neither hang nor bleed into the next run
    (
      b"\x1b[31mab\x1b[0mcd",
      vec![hl(0..2)],
      0..4,
      format!("{reset}{red}{bg}ab{reset}cd"),
    ),
    // clipped by the row: starts before it and ends after it
    (
      b"abcdef",
      vec![hl(1..5)],
      2..4,
      format!("{reset}{bg}cd{reset}"),
    ),
  ];

  for (raw, highlights, range, expected) in cases {
    let line = parsed(raw);
    let mut out = Vec::new();
    line
      .render_range(&mut out, range.clone(), &highlights)
      .unwrap();
    assert_eq!(
      String::from_utf8(out).unwrap(),
      expected,
      "failed on {raw:?} {highlights:?} {range:?}"
    );
  }
}

#[test]
fn plain_text_only_accepts_lines_parse_would_not_change() {
  let cases: [(&[u8], bool); 11] = [
    (b"hello world", true),
    (b"hello world\n", true),
    (b"hello world\r\n", true),
    (b"", true),
    (b"a\tb", false),
    (b"\x1b[31mred\x1b[0m", false),
    (b"bold\x08b", false),
    (b"caf\xc3\xa9", false),
    (b"ctrl\x01", false),
    (b"del\x7f", false),
    (b"mid\rdle", false),
  ];

  for (raw, plain) in cases {
    let text = plain_text(raw);
    assert_eq!(text.is_some(), plain, "failed on {raw:?}");
    if let Some(text) = text {
      // the fast path must agree with the real parser
      assert_eq!(text, parsed(raw).text(), "failed on {raw:?}");
    }
  }
}

#[test]
fn verbatim_block_only_accepts_blocks_parse_would_not_change() {
  // 130 bytes = two 64-byte chunks plus a 2-byte remainder, with `byte` planted at `at`.
  let planted = |at: usize, byte: u8| {
    let mut block = vec![b'a'; 130];
    if let Some(slot) = block.get_mut(at) {
      *slot = byte;
    }
    block
  };
  let cases = [
    (b"hello\nworld\n".to_vec(), true),
    (b"".to_vec(), true),
    // accents survive parsing unchanged, so they must not defeat the fast path
    (b"caf\xc3\xa9\nna\xc3\xafve\n".to_vec(), true),
    // U+00A0 shares the `C2` lead byte with the C1 controls but is not one
    (b"nbsp\xc2\xa0\n".to_vec(), true),
    (b"a\tb\n".to_vec(), false),
    (b"\x1b[31mred\x1b[0m\n".to_vec(), false),
    (b"bold\x08b\n".to_vec(), false),
    (b"del\x7f\n".to_vec(), false),
    // the `\r` of a CRLF ending is stripped by parse, so it is allowed...
    (b"crlf\r\nmore\r\n".to_vec(), true),
    (b"\r\n".to_vec(), true),
    // ...including when the pair straddles a chunk edge (`\r` at 63, `\n` at 64)
    (
      [vec![b'a'; 63], b"\r\n".to_vec(), vec![b'a'; 65]].concat(),
      true,
    ),
    // any other `\r` is shown as `^M`
    (b"mid\rdle\n".to_vec(), false),
    (b"doubled\r\r\n".to_vec(), false),
    (b"unfinished\r".to_vec(), false),
    (planted(63, b'\r'), false),
    (planted(129, b'\r'), false),
    // C1 controls become U+FFFD
    (b"nel\xc2\x85\n".to_vec(), false),
    (b"csi\xc2\x9b\n".to_vec(), false),
    (b"invalid\xff\n".to_vec(), false),
    // a control byte anywhere, including either side of a chunk edge and in the remainder
    (planted(0, b'\t'), false),
    (planted(63, b'\x1b'), false),
    (planted(64, b'\x1b'), false),
    (planted(128, b'\t'), false),
    (planted(129, b'\x7f'), false),
  ];

  for (raw, verbatim) in cases {
    let text = verbatim_block(&raw);
    assert_eq!(text.is_some(), verbatim, "failed on {raw:?}");
    if let Some(text) = text {
      // every line in an accepted block must parse to itself
      for line in text.split_inclusive('\n') {
        let expected = line.strip_suffix('\n').unwrap_or(line);
        let expected = expected.strip_suffix('\r').unwrap_or(expected);
        assert_eq!(
          parsed(line.as_bytes()).text(),
          expected,
          "failed on {line:?}"
        );
      }
    }
  }
}
