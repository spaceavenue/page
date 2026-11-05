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
  Help,
  Quit,
}

macro_rules! keymap {
  (@key $c:literal) => { KeyCode::Char($c) };
  (@key $k:ident)   => { KeyCode::$k };
  (@mods) => { KeyModifiers::NONE };
  (@mods $($m:ident)|+) => { KeyModifiers::NONE $(.union(KeyModifiers::$m))+ };

  (
    $vis:vis $name:ident:
    $(
      [$($code:tt),+ $(,)?] $(with $($m:ident)|+)? => $action:ident
    ),+ $(,)?
  ) => {
    $vis const $name: &[(&[KeyCode], KeyModifiers, Action,)] = &[
      $((
        &[$(keymap! (@key $code)),+],
        keymap! (@mods $($($m)|+)?),
        Action::$action,
      )),+
    ];
  };
}

keymap! {
    KEYMAP:
    ['q', Esc]            => Quit,
    ['c'] with CONTROL    => Quit,
    [Up, 'k']             => LineUp,
    [Down, 'j', Enter]    => LineDown,
    [PageUp]              => PageUp,
    ['b'] with CONTROL    => PageUp,
    [PageDown, ' ']       => PageDown,
    ['f'] with CONTROL    => PageDown,
    ['u'] with CONTROL    => HalfPageUp,
    ['d'] with CONTROL    => HalfPageDown,
    ['g', Home]           => GotoTop,
    ['G', End]            => GotoBottom,
    [Left]                => ScrollLeft,
    [Right]               => ScrollRight,
    ['0']                 => ScrollHome,
    ['h', '?']            => Help,
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
    .find(|(codes, map_mods, _)| *map_mods == mods && codes.contains(&key.code))
    .map(|(.., action)| *action)
}
