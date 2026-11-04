mod buffer;
mod config;
mod pager;
mod scroll;
mod text;
mod ui;

use buffer::Buffer;
use config::cli::Cli;
use pager::Pager;
use ui::terminal::TerminalGuard;

fn main() -> anyhow::Result<()> {
  let args = Cli::parse()?;
  let buffer = Buffer::load(&args.input)?;
  let _guard = TerminalGuard::init()?;
  Pager::new(buffer, args.options)?.run()
}
