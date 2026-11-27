use std::iter::{self, Peekable};
use std::num::NonZeroUsize;
use std::str::Chars;

use crate::config::options::Options;
use crate::search::settings::SearchCase;
use crate::text::WrapMode;

/// Widest accepted tab stop. Matches `--tab-width`.
const MAX_TAB_WIDTH: usize = 256;

/// Parse `$LESS` and apply recognized options from it.
pub fn apply(options: &mut Options, value: &str) {
  let mut chars = value.chars().peekable();
  while let Some(c) = chars.next() {
    match c {
      // a startup command such as `+F` runs to the next `$`
      '+' => {
        chars.find(|&c| c == '$');
      },
      '-' if chars.next_if_eq(&'-').is_some() => long_option(options, &mut chars),
      // `-+X` resets an option in `less` but we dont care
      '-' if chars.next_if_eq(&'+').is_some() => {
        chars.next();
      },
      // the dash is optional and letters can be bundled, so everything else is an option letter
      letter => short_option(options, letter, &mut chars),
    }
  }
}

/// Apply the option `letter`. Consumes its argument from `chars` if it takes one.
fn short_option(options: &mut Options, letter: char, chars: &mut Peekable<Chars>) {
  match letter {
    'F' => options.print_if_fits = true,
    'N' => options.line_numbers = true,
    'S' => options.wrap = WrapMode::None,
    'i' => options.search_settings.case = SearchCase::Smart,
    'I' => options.search_settings.case = SearchCase::Insensitive,
    'x' => {
      if let Some(width) = take_tab_width(chars) {
        options.tab_width = width;
      }
    },
    // these take a string which may contain spaces and ends at `$`
    'P' | 'p' | 't' | 'T' | 'k' | 'o' | 'O' | 'D' | '"' => {
      chars.find(|&c| c == '$');
    },
    // separators, unrecognized flags, digits of numeric arguments
    _ => {},
  }
}

/// Apply the long option following `--`.
fn long_option(options: &mut Options, chars: &mut Peekable<Chars>) {
  let name: String =
    iter::from_fn(|| chars.next_if(|&c| !matches!(c, ' ' | '\t' | '=' | '$'))).collect();
  let letter = match name.as_str() {
    "quit-if-one-screen" => 'F',
    "LINE-NUMBERS" => 'N',
    "chop-long-lines" => 'S',
    "ignore-case" => 'i',
    "IGNORE-CASE" => 'I',
    "tabs" => 'x',
    // any option taking a string behaves like `-P`: skip to the `$`
    "color" | "lesskey-file" | "log-file" | "LOG-FILE" | "pattern" | "prompt" | "quotes"
    | "tag" | "tag-file" => 'P',
    _ => {
      if chars.next_if_eq(&'=').is_some() {
        chars.find(|&c| matches!(c, ' ' | '\t' | '$'));
      }
      return;
    },
  };
  chars.next_if_eq(&'=');
  short_option(options, letter, chars);
}

/// Consume a tab width. Returns `None` if missing, zero or too wide.
fn take_tab_width(chars: &mut Peekable<Chars>) -> Option<NonZeroUsize> {
  while chars.next_if_eq(&' ').is_some() {}
  let digits: String = iter::from_fn(|| chars.next_if(char::is_ascii_digit)).collect();
  digits
    .parse()
    .ok()
    .filter(|width: &NonZeroUsize| width.get() <= MAX_TAB_WIDTH)
}
