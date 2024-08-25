mod daemon;
mod mush;
mod wasi;
use clap::{Parser, Subcommand};
pub use daemon::daemon;
use fungi_config::{FungiDir, DEFAULT_FUNGI_DIR};
use fungi_daemon::DaemonArgs;
use libp2p::PeerId;
pub use mush::mush;
use std::path::PathBuf;
pub use wasi::wasi;

/// Fungi the world!
#[derive(Parser, Debug, Clone)]
#[command(version, about, long_about = None)]
pub struct FungiArgs {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Parser, Debug, Clone)]
pub struct WasiArgs {
    #[arg(short, long)]
    pub fungi_dir: Option<String>,
}

#[derive(Parser, Debug, Clone)]
pub struct MushArgs {
    #[arg(short, long)]
    pub fungi_dir: Option<String>,

    /// Connect to a remote peer
    #[arg(short, long)]
    pub peer: Option<PeerId>,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Start a Fungi daemon
    Daemon(DaemonArgs),

    /// Start a Fungi wasi process
    Wasi(WasiArgs),

    /// Start a Fungi mush process
    Mush(MushArgs),
}

impl FungiDir for WasiArgs {
    fn fungi_dir(&self) -> PathBuf {
        self.fungi_dir
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| home::home_dir().unwrap().join(DEFAULT_FUNGI_DIR))
    }
}

impl FungiDir for MushArgs {
    fn fungi_dir(&self) -> PathBuf {
        self.fungi_dir
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| home::home_dir().unwrap().join(DEFAULT_FUNGI_DIR))
    }
}
