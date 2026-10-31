use crossterm::style::{Attribute, Attributes, Color};

use crate::reader::Reader;

const ESC: char = '\x1b';
const BEL: char = '\x07';

/// The visual style of a run of text.
///
/// `None` means "terminal default", which is different from [`Color::Reset`], which is is emitted
/// explicitly when a run ends.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Style {
  pub fg: Option<Color>,
  pub bg: Option<Color>,
  pub attrs: Attributes,
}

/// Consume one escape sequence; the reader must be positioned on `ESC`.
///
/// Returns the style after applying the sequence.
#[must_use]
pub fn escape(reader: &mut Reader, enabled: bool, style: Style) -> Style {
  reader.bump();

  let Some(kind) = reader.bump() else {
    return style;
  };

  match kind {
    '[' => csi(reader, enabled, style),
    ']' => {
      skip_string(reader, true);
      style
    }
    'P' | '_' | '^' | 'X' => {
      skip_string(reader, false);
      style
    }
    // Character-set designations and similar. one more byte follows.
    '(' | ')' | '*' | '+' | '-' | '.' | '/' => {
      reader.bump();
      style
    }
    _ => style,
  }
}

fn csi(reader: &mut Reader, enabled: bool, style: Style) -> Style {
  let params = reader.read_while(|ch| matches!(ch, ' '..='?'));
  match reader.peek() {
    Some('m') if enabled => {
      reader.bump();
      apply_sgr(style, params)
    }
    Some('@'..='~') => {
      reader.bump();
      style
    }
    _ => style,
  }
}

/// Skip an OSC/DCS/APC/PM string.
fn skip_string(reader: &mut Reader, allow_bel: bool) {
  while let Some(ch) = reader.bump() {
    if (allow_bel && ch == BEL) || (ch == ESC && reader.eat('\\')) {
      break;
    }
  }
}

/// Apply the parameters of an SGR sequence (the part between `ESC [` and `m`) to `style`.
///
/// `;` separates parameters and `:` separates sub-parameters within a parameter. An empty
/// parameter counts as `0`, so `ESC[m` and `ESC[1;m` both end with a reset.
#[must_use]
pub fn apply_sgr(mut style: Style, params: &str) -> Style {
  let mut groups = params.split(';');
  while let Some(group) = groups.next() {
    let mut sub = group.split(':');
    let Some(code) = sub.next().and_then(parse_number) else {
      continue;
    };

    match code {
      0 => style = Style::default(),
      4 => match sub.next().map(parse_number) {
        Some(Some(0)) => style.attrs.unset(Attribute::Underlined),
        _ => style.attrs.set(Attribute::Underlined),
      },
      38 | 48 => {
        let color = if group.contains(':') {
          color_from_subparams(&mut sub)
        } else {
          color_from_params(&mut groups)
        };
        if let Some(color) = color {
          if code == 38 {
            style.fg = Some(color);
          } else {
            style.bg = Some(color);
          }
        }
      }
      _ => apply_simple(&mut style, code),
    }
  }
  style
}

fn apply_simple(style: &mut Style, code: u16) {
  match code {
    // 1..=9, 20, 51..=53: Enable attributes
    1 => style.attrs.set(Attribute::Bold),
    2 => style.attrs.set(Attribute::Dim),
    3 => style.attrs.set(Attribute::Italic),
    5 => style.attrs.set(Attribute::SlowBlink),
    6 => style.attrs.set(Attribute::RapidBlink),
    7 => style.attrs.set(Attribute::Reverse),
    8 => style.attrs.set(Attribute::Hidden),
    9 => style.attrs.set(Attribute::CrossedOut),
    20 => style.attrs.set(Attribute::Fraktur),
    51 => style.attrs.set(Attribute::Framed),
    52 => style.attrs.set(Attribute::Encircled),
    53 => style.attrs.set(Attribute::OverLined),

    // 21..=29, 54..=55: Disable attributes
    21 => style.attrs.unset(Attribute::Bold),
    22 => {
      style.attrs.unset(Attribute::Bold);
      style.attrs.unset(Attribute::Dim);
    }
    23 => style.attrs.unset(Attribute::Italic),
    24 => style.attrs.unset(Attribute::Underlined),
    25 => {
      style.attrs.unset(Attribute::SlowBlink);
      style.attrs.unset(Attribute::RapidBlink);
    }
    27 => style.attrs.unset(Attribute::Reverse),
    28 => style.attrs.unset(Attribute::Hidden),
    29 => style.attrs.unset(Attribute::CrossedOut),
    54 => {
      style.attrs.unset(Attribute::Framed);
      style.attrs.unset(Attribute::Encircled);
    }
    55 => style.attrs.unset(Attribute::OverLined),

    // colors
    30..=37 => style.fg = Some(basic_color(code - 30)),
    39 => style.fg = None,
    40..=47 => style.bg = Some(basic_color(code - 40)),
    49 => style.bg = None,
    90..=97 => style.fg = Some(bright_color(code - 90)),
    100..=107 => style.bg = Some(bright_color(code - 100)),
    _ => {}
  }
}

/// Parse `38;5;n` / `38;2;r;g;b`, where the color parameters are separate semicolon-delimited SGR
/// parameters.
fn color_from_params<'a>(groups: &mut impl Iterator<Item = &'a str>) -> Option<Color> {
  match parse_number(groups.next()?)? {
    5 => parse_u8(groups.next()?).map(Color::AnsiValue),
    2 => {
      let r = groups.next().and_then(parse_u8);
      let g = groups.next().and_then(parse_u8);
      let b = groups.next().and_then(parse_u8);

      Some(Color::Rgb {
        r: r?,
        g: g?,
        b: b?,
      })
    }
    _ => None,
  }
}

/// Parse `38:5:n` / `38:2:r:g:b` / `38:2::r:g:b`, where the color parameters are colon-delimited
/// subparameters of a single SGR parameter.
fn color_from_subparams<'a>(sub: &mut impl Iterator<Item = &'a str>) -> Option<Color> {
  match parse_number(sub.next()?)? {
    5 => parse_u8(sub.next()?).map(Color::AnsiValue),
    2 => {
      // An optional color-space identifier may sit between the subtype and the components, so
      // either three values (`r:g:b`) or four (`cs:r:g:b`) follow.
      let mut values = sub.map(parse_u8);
      let first = values.next()?;
      let second = values.next()?;
      let third = values.next()?;

      let (r, g, b) = values
        .next()
        .map_or((first, second, third), |fourth| (second, third, fourth));

      Some(Color::Rgb {
        r: r?,
        g: g?,
        b: b?,
      })
    }
    _ => None,
  }
}

fn parse_number(text: &str) -> Option<u16> {
  if text.is_empty() {
    return Some(0);
  }
  if !text.bytes().all(|b| b.is_ascii_digit()) {
    return None;
  }
  text.parse().ok()
}

fn parse_u8(bytes: &str) -> Option<u8> {
  u8::try_from(parse_number(bytes)?).ok()
}

const fn basic_color(index: u16) -> Color {
  match index {
    0 => Color::Black,
    1 => Color::DarkRed,
    2 => Color::DarkGreen,
    3 => Color::DarkYellow,
    4 => Color::DarkBlue,
    5 => Color::DarkMagenta,
    6 => Color::DarkCyan,
    _ => Color::Grey,
  }
}

const fn bright_color(index: u16) -> Color {
  match index {
    0 => Color::DarkGrey,
    1 => Color::Red,
    2 => Color::Green,
    3 => Color::Yellow,
    4 => Color::Blue,
    5 => Color::Magenta,
    6 => Color::Cyan,
    _ => Color::White,
  }
}
