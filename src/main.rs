use page_rs::buffer::Buffer;
use page_rs::config::cli::Cli;
use page_rs::pager::Pager;
use page_rs::ui::terminal::TerminalGuard;

fn main() -> anyhow::Result<()> {
  let Some(args) = Cli::parse()? else {
    return Ok(());
  };
  let buffer = Buffer::load(&args.input)?;
  let _guard = TerminalGuard::init()?;
  Pager::new(buffer, args.options)?.run()
}
