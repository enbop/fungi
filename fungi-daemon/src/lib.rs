mod daemon;
pub use daemon::FungiDaemon;
use std::path::PathBuf;
pub mod listeners;
use fungi_config::{FungiDir, DEFAULT_FUNGI_DIR};
use tokio::sync::OnceCell;
#[cfg(feature = "cli")]
use clap::Parser;

pub static ALL_IN_ONE_BINARY: OnceCell<bool> = OnceCell::const_new();

#[derive(Debug, Clone)]
#[cfg_attr(feature = "cli", derive(Parser))]
pub struct DaemonArgs {
    #[cfg_attr(feature = "cli", arg(short, long))]
    pub fungi_dir: Option<String>,

    #[cfg_attr(feature = "cli", arg(long))]
    pub wasi_bin_path: Option<String>,

    /// DEBUG ONLY: Allow all inbound connections
    #[cfg_attr(feature = "cli", arg(long))]
    pub debug_allow_all_peers: Option<bool>,
}

impl FungiDir for DaemonArgs {
    fn fungi_dir(&self) -> PathBuf {
        self.fungi_dir
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| home::home_dir().unwrap().join(DEFAULT_FUNGI_DIR))
    }
}
