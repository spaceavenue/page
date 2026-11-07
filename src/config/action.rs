use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// User-facing actions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
  LineUp,
  LineDown,
  PageUp,
  PageDown,
  HalfPageUp,
  HalfPageDown,
  GotoTop,
  GotoBottom,
  ScrollLeft,
  ScrollRight,
  ScrollHome,
  SearchStart,
  SearchForward,
  SearchBackward,
  SearchModeToggle,
  Help,
  Quit,
}

/// Defines a keymap as a slice of `(Action, &[(KeyModifiers, KeyCode)])`.
///
/// Each action lists any number of keys, separated by commas. A key is either a bare code or one or
/// more modifiers joined with `|` followed by `+` and a code.
macro_rules! keymap {
  (@key $c:literal) => { KeyCode::Char($c) };
  (@key $k:ident)   => { KeyCode::$k };

  // Key list muncher: accumulates `(mods, code),` pairs, then emits them as a slice.
  (@keys [$($acc:tt)*]) => { &[$($acc)*] };
  (@keys [$($acc:tt)*] $($m:ident)|+ + $c:tt $(, $($rest:tt)*)?) => {
    keymap!(
      @keys
      [$($acc)* (KeyModifiers::NONE $(.union(KeyModifiers::$m))+, keymap!(@key $c)),]
      $($($rest)*)?
    )
  };
  (@keys [$($acc:tt)*] $c:tt $(, $($rest:tt)*)?) => {
    keymap!(
      @keys
      [$($acc)* (KeyModifiers::NONE, keymap!(@key $c)),]
      $($($rest)*)?
    )
  };

  (
    $vis:vis $name:ident:
    $($action:ident => [$($keys:tt)*]),+ $(,)?
  ) => {
    $vis const $name: &[(Action, &[(KeyModifiers, KeyCode)])] = &[
      $((Action::$action, keymap!(@keys [] $($keys)*))),+
    ];
  };
}

keymap! {
    KEYMAP:
    Quit           => ['q', Esc, CONTROL + 'c'],
    LineUp         => [Up, 'k'],
    LineDown       => [Down, 'j', Enter],
    PageUp         => [PageUp, CONTROL + 'b'],
    PageDown       => [PageDown, ' ', CONTROL + 'f'],
    HalfPageUp     => [CONTROL + 'u'],
    HalfPageDown   => [CONTROL + 'd'],
    GotoTop        => ['g', Home],
    GotoBottom     => ['G', End],
    ScrollLeft     => [Left],
    ScrollRight    => [Right],
    ScrollHome     => ['0'],
    SearchStart    => ['/', CONTROL + 'f'],
    SearchForward  => ['n'],
    SearchBackward => ['N'],
    SearchModeToggle  => ['M'],
    Help           => ['h', '?'],
}

#[must_use]
pub fn action_for_key(key: KeyEvent) -> Option<Action> {
  let mods = match key.code {
    // a caveat here is crossterm will send, for eg, Shift+g as Char(G), KeyModifiers::SHIFT. we can
    // just match on capital letters, so remove the Shift modifier.
    KeyCode::Char(_) => key.modifiers - KeyModifiers::SHIFT,
    _ => key.modifiers,
  };

  KEYMAP
    .iter()
    .find(|(_, keys)| keys.contains(&(mods, key.code)))
    .map(|(action, _)| *action)
}
