mod controls;
mod daemon;
pub mod listeners;

#[cfg(feature = "cli")]
use clap::Parser;
pub use daemon::FungiDaemon;
use fungi_config::{DEFAULT_FUNGI_DIR, FungiDir};
use std::path::PathBuf;

#[derive(Debug, Clone)]
#[cfg_attr(feature = "cli", derive(Parser))]
pub struct DaemonArgs {
    #[cfg_attr(feature = "cli", arg(short, long))]
    pub fungi_dir: Option<String>,

    #[cfg_attr(feature = "cli", arg(long))]
    pub fungi_bin_path: Option<String>,

    /// DEBUG ONLY: Allow all inbound connections
    #[cfg_attr(feature = "cli", arg(long))]
    pub debug_allow_all_peers: Option<bool>,
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
