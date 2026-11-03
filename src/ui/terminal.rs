use std::io::{Result, Stdout, stdout};

use crossterm::terminal::{
  EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use crossterm::{cursor, execute};

pub struct TerminalGuard;

impl TerminalGuard {
  /// Enter raw mode and alternate screen, installing a panic hook to restore terminal state.
  ///
  /// # Errors
  ///
  /// Returns an error if enabling raw mode or switching to alternate screen fails.
  pub fn init() -> anyhow::Result<Self> {
    enable_raw_mode()?;

    // panic hook to restore terminal on panic
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
      let _ = Self::restore_terminal(&mut std::io::stdout());
      default_panic(info);
    }));

    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, cursor::Hide)?;
    Ok(Self)
  }

  fn restore_terminal(stdout: &mut Stdout) -> Result<()> {
    execute!(stdout, LeaveAlternateScreen, cursor::Show)?;
    disable_raw_mode()
  }
}

impl Drop for TerminalGuard {
  fn drop(&mut self) {
    let mut stdout = stdout();
    let _ = Self::restore_terminal(&mut stdout);
  }
}
