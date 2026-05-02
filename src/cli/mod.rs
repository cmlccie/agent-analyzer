pub mod args;
pub mod log;
pub mod serve;
pub mod status;
pub mod tui;

pub use args::Args;

use args::Command;
use clap::Parser;

pub async fn run() -> anyhow::Result<()> {
    let args = Args::parse();
    log::init(args.verbose);

    match args.command {
        Some(Command::Serve(serve_args)) => serve::run(serve_args).await,
        Some(Command::Tui(tui_args)) => {
            // TUI runs on the current (tokio) thread; spawn the polling task
            // on the same runtime before entering the blocking event loop.
            tui::run(tui_args)
        }
        Some(Command::Status(status_args)) => status::run(status_args).await,
        // Default: launch TUI against localhost.
        None => {
            let tui_args = args::TuiArgs {
                url: "http://localhost:8000".parse().unwrap(),
            };
            tui::run(tui_args)
        }
    }
}
