mod buffer;
mod config;
mod output;
mod pager;
mod scroll;
mod search;
mod text;
mod ui;

use std::io::{IsTerminal, stdout};

use anyhow::Context;
use buffer::Buffer;
use config::cli::Cli;
use crossterm::terminal;
use pager::Pager;
use ui::terminal::TerminalGuard;

use crate::ui::viewport::Viewport;

fn main() -> anyhow::Result<()> {
  let args = Cli::parse();
  if !stdout().is_terminal() {
    output::passthrough(&args.input, &mut stdout().lock())
      .with_context(|| format!("failed to read path: {}", args.input))?;
    return Ok(());
  }
  let mut buffer = Buffer::load(&args.input)?;
  if args.options.print_if_fits {
    let term_size = terminal::size()?;
    let (width, height) = (term_size.0.into(), term_size.1.into());
    let viewport = Viewport::new(width, height);
    buffer.wait_for_lines(height)?;
    if output::fits(&mut buffer, &args.options, viewport) {
      return pager::print_screen(&buffer, &args.options, viewport, &mut stdout().lock());
    }
  }
  let _guard = TerminalGuard::init()?;
  Pager::new(buffer, args.input.to_string(), args.options)?.run()
}
