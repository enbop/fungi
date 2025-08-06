mod api;
mod controls;
mod daemon;
pub mod listeners;

use clap::Parser;
pub use daemon::FungiDaemon;
use fungi_config::{DEFAULT_FUNGI_DIR, FungiDir};
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Parser)]
pub struct DaemonArgs {
    #[clap(short, long, help = "Path to the Fungi directory")]
    pub fungi_dir: Option<String>,
}

impl DaemonArgs {}

impl FungiDir for DaemonArgs {
    fn fungi_dir(&self) -> PathBuf {
        self.fungi_dir
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| home::home_dir().unwrap().join(DEFAULT_FUNGI_DIR))
    }
}
