mod api;
mod controls;
mod daemon;

use clap::Parser;
pub use daemon::FungiDaemon;
use fungi_config::{DEFAULT_FUNGI_DIR, FungiDir};
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Parser)]
pub struct DaemonArgs {
    #[clap(
        short,
        long,
        help = "Path to the Fungi config directory, defaults to ~/.fungi"
    )]
    pub fungi_dir: Option<String>,

    #[clap(
        long,
        help = "Exit when stdin is closed (useful when running as a subprocess)"
    )]
    pub exit_on_stdin_close: bool,
}

impl FungiDir for DaemonArgs {
    fn fungi_dir(&self) -> PathBuf {
        self.fungi_dir
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| home::home_dir().unwrap().join(DEFAULT_FUNGI_DIR))
    }
}
