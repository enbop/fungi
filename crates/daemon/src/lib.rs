mod api;
mod controls;
mod daemon;

use clap::Parser;
pub use daemon::FungiDaemon;

#[derive(Debug, Clone, Default, Parser)]
pub struct DaemonArgs {
    #[clap(
        long,
        help = "Exit when stdin is closed (useful when running as a subprocess)"
    )]
    pub exit_on_stdin_close: bool,
}
