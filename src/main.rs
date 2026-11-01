use page::buffer::Buffer;
use page::config::cli::Cli;
use page::pager::Pager;
use page::ui::terminal::TerminalGuard;

fn main() -> anyhow::Result<()> {
  let args = Cli::parse()?;
  let buffer = Buffer::load(&args.input)?;
  let _guard = TerminalGuard::init()?;
  Pager::new(buffer, args.options)?.run()
}
