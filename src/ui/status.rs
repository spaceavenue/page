use std::fmt::{self, Display, Formatter, Write as _};

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::text::reader::Reader;

const DEFAULT_TEMPLATE: &str = "{filename} {lines}{plus} lines {percent}% {state}";

/// Values available to the status-bar template.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StatusContext<'a> {
  pub filename: &'a str,
  pub lines: usize,
  pub percent: usize,
  pub reading: bool,
}

impl<'a> StatusContext<'a> {
  #[must_use]
  pub const fn new(filename: &'a str, lines: usize, percent: usize, reading: bool) -> Self {
    Self {
      filename,
      lines,
      percent,
      reading,
    }
  }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Piece {
  Literal(Box<str>),
  Token { kind: TokenKind, spec: FormatSpec },
}

/// Compiled status-bar template.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusTemplate {
  pieces: Vec<Piece>,
}

/// A token that can appear in a status template.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TokenKind {
  Filename,
  Lines,
  Plus,
  Percent,
  State,
}

impl TokenKind {
  fn parse(name: &str) -> Option<Self> {
    Some(match name {
      "filename" => Self::Filename,
      "lines" => Self::Lines,
      "plus" => Self::Plus,
      "percent" => Self::Percent,
      "state" => Self::State,
      _ => return None,
    })
  }

  const fn value<'a>(self, context: &StatusContext<'a>) -> Value<'a> {
    match self {
      Self::Filename => Value::Text(context.filename),
      Self::Lines => Value::Number(context.lines),
      Self::Plus => Value::Text(if context.reading { "+" } else { "" }),
      Self::Percent => Value::Number(context.percent),
      Self::State => Value::Text(if context.reading { "[reading...]" } else { "" }),
    }
  }
}

#[derive(Clone, Copy, Debug)]
enum Value<'a> {
  Text(&'a str),
  Number(usize),
}

impl Value<'_> {
  const fn is_count(self) -> bool {
    matches!(self, Self::Number(_))
  }

  fn width(self) -> usize {
    match self {
      Self::Text(text) => text_width(text),
      Self::Number(number) => count_digits(number),
    }
  }

  fn write(self, out: &mut String) -> fmt::Result {
    match self {
      Self::Text(text) => {
        out.push_str(text);
        Ok(())
      },
      Self::Number(number) => write!(out, "{number}"),
    }
  }
}

/// Formatting applied to one token.
///
/// Only alignment is supported for now. `width` is the minimum number of display columns; values
/// wider than the width are not truncated.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct FormatSpec {
  align: Option<Align>,
  width: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Align {
  Left,
  Right,
  Center,
}

impl Align {
  const fn parse(ch: char) -> Option<Self> {
    match ch {
      '<' => Some(Self::Left),
      '>' => Some(Self::Right),
      '^' => Some(Self::Center),
      _ => None,
    }
  }
}

/// An error produced while compiling a status template.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusError {
  offset: usize,
  message: String,
}

impl StatusError {
  fn new(offset: usize, message: impl Into<String>) -> Self {
    Self {
      offset,
      message: message.into(),
    }
  }

  /// Byte offset in the template where the error starts.
  #[must_use]
  pub const fn offset(&self) -> usize {
    self.offset
  }

  /// Human-readable error message.
  #[must_use]
  pub fn message(&self) -> &str {
    &self.message
  }
}

impl Display for StatusError {
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    write!(f, "{} at byte {}", self.message(), self.offset())
  }
}

impl std::error::Error for StatusError {}

impl Default for StatusTemplate {
  #[allow(clippy::expect_used)] // constant template, covered by the status tests
  fn default() -> Self {
    Self::compile(DEFAULT_TEMPLATE).expect("default status template is valid")
  }
}

fn err<T>(offset: usize, msg: &str) -> Result<T, StatusError> {
  Err(StatusError::new(offset, msg))
}

impl StatusTemplate {
  /// Compile a status template into literal and token pieces.
  ///
  /// # Errors
  ///
  /// Returns [`StatusError`] for an unknown token, an unclosed `{`, a lone `}`, an invalid format
  /// spec, or a control character in literal text.
  pub fn compile(source: &str) -> Result<Self, StatusError> {
    let mut pieces = Vec::new();
    let mut literal = String::new();
    let mut reader = Reader::new(source);

    let offset_of = |reader: &Reader<'_>| source.len() - reader.remaining().len();

    while !reader.remaining().is_empty() {
      let chunk = reader.read_while(|ch| ch != '{' && ch != '}' && !ch.is_control());
      literal.push_str(chunk);

      let Some(ch) = reader.peek() else {
        break;
      };

      let offset = offset_of(&reader);
      match ch {
        '{' => {
          reader.bump();
          if reader.eat('{') {
            literal.push('{');
            continue;
          }

          flush_literal(&mut pieces, &mut literal);
          let start = offset_of(&reader);

          let body = reader.read_while(|ch| ch != '{' && ch != '}');

          if reader.peek() == Some('{') {
            return err(offset_of(&reader), "nested `{` in token");
          }

          if !reader.eat('}') {
            return err(offset, "unclosed `{}`");
          }

          let (name, spec) = body
            .split_once(':')
            .map_or((body, None), |(name, spec)| (name, Some(spec)));

          if name.is_empty() {
            return err(start, "empty token name");
          }

          let Some(kind) = TokenKind::parse(name) else {
            return err(start, &format!("unknown token `{name}`"));
          };

          let spec = match spec {
            Some(spec) => parse_spec(spec, start + name.len() + 1)?,
            None => FormatSpec::default(),
          };
          pieces.push(Piece::Token { kind, spec });
        },
        '}' => {
          reader.bump();
          if reader.eat('}') {
            literal.push('}');
          } else {
            return err(offset, "unexpected `}`");
          }
        },
        ch if ch.is_control() => {
          return err(offset, "control character in literal text");
        },
        _ => return err(offset_of(&reader), "unknown symbol reached, somehow."),
      }
    }

    flush_literal(&mut pieces, &mut literal);
    Ok(Self { pieces })
  }

  /// Render this template into `out`.
  ///
  /// # Errors
  ///
  /// Returns a formatting error if writing to `out` fails. Writing to a `String` cannot fail in
  /// practice.
  pub fn render(&self, context: &StatusContext<'_>, out: &mut String) -> fmt::Result {
    for piece in &self.pieces {
      match piece {
        Piece::Literal(text) => out.push_str(text),
        Piece::Token { kind, spec } => write_value(out, kind.value(context), *spec)?,
      }
    }
    Ok(())
  }
}

fn flush_literal(pieces: &mut Vec<Piece>, literal: &mut String) {
  if literal.is_empty() {
    return;
  }
  pieces.push(Piece::Literal(std::mem::take(literal).into_boxed_str()));
}

fn parse_spec(spec: &str, offset: usize) -> Result<FormatSpec, StatusError> {
  if spec.is_empty() {
    return Ok(FormatSpec::default());
  }

  let mut chars = spec.chars();
  let align = chars.next().and_then(Align::parse);
  let rest = if align.is_some() {
    chars.as_str()
  } else {
    spec
  };

  if rest.is_empty() {
    return Ok(FormatSpec { align, width: None });
  }

  if !rest.bytes().all(|byte| byte.is_ascii_digit()) {
    return err(offset, "invalid format spec");
  }

  let Ok(width) = rest.parse() else {
    return err(offset, "format width is too large");
  };

  if width == 0 {
    return err(offset, "format width must be positive");
  }

  Ok(FormatSpec {
    align,
    width: Some(width),
  })
}

fn write_value(out: &mut String, value: Value<'_>, spec: FormatSpec) -> fmt::Result {
  let width = value.width();
  let default_align = if value.is_count() {
    Align::Right
  } else {
    Align::Left
  };
  let align = spec.align.unwrap_or(default_align);
  let pad = spec.width.unwrap_or(0).saturating_sub(width);
  let (left, right) = match align {
    Align::Left => (0, pad),
    Align::Right => (pad, 0),
    Align::Center => (pad / 2, pad - pad / 2),
  };
  out.extend(std::iter::repeat_n(' ', left));
  value.write(out)?;
  out.extend(std::iter::repeat_n(' ', right));
  Ok(())
}

fn text_width(text: &str) -> usize {
  text
    .graphemes(true)
    .map(|grapheme| {
      if grapheme.chars().any(char::is_control) {
        1
      } else {
        UnicodeWidthStr::width(grapheme)
      }
    })
    .sum()
}

fn count_digits(number: usize) -> usize {
  number.checked_ilog10().unwrap_or(0) as usize
}
