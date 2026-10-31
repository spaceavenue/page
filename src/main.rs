use page_rs::buffer::Buffer;
use page_rs::cli::Cli;
use page_rs::pager::{Options, Pager};
use page_rs::terminal::TerminalGuard;

fn main() -> anyhow::Result<()> {
  let Some(args) = Cli::parse()? else {
    return Ok(());
  };
  let buffer = Buffer::load(&args.input)?;
  let options = Options {
    follow: args.follow,
    hard_wrap: args.hard_wrap,
    ansi: args.ansi,
    tab_width: args.tab_width,
  };
  let _guard = TerminalGuard::init()?;
  Pager::new(buffer, options)?.run()
}
