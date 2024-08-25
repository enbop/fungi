pub mod daemon;
mod init;
mod mush;
mod wasi;
pub use daemon::daemon;
pub use init::init;
use libp2p::PeerId;
pub use mush::mush;
pub use wasi::wasi;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

use crate::{
    DEFAULT_FUNGI_DIR, DEFAULT_FUNGI_WASI_BIN_DIR_NAME, DEFAULT_FUNGI_WASI_ROOT_DIR_NAME,
    DEFAULT_IPC_DIR_NAME, MUSH_LISTENER_ADDR,
};

/// Fungi the world!
#[derive(Parser, Debug, Clone)]
#[command(version, about, long_about = None)]
pub struct FungiArgs {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Parser, Debug, Clone)]
pub struct DaemonArgs {
    #[arg(short, long)]
    pub fungi_dir: Option<String>,

    #[arg(long)]
    pub wasi_bin_path: Option<String>,

    /// DEBUG ONLY: Allow all inbound connections
    #[arg(long)]
    pub debug_allow_all_peers: Option<bool>,
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

pub trait FungiDir {
    fn fungi_dir(&self) -> PathBuf;

    fn wasi_root_dir(&self) -> PathBuf {
        self.fungi_dir().join(DEFAULT_FUNGI_WASI_ROOT_DIR_NAME)
    }

    fn wasi_bin_dir(&self) -> PathBuf {
        self.wasi_root_dir().join(DEFAULT_FUNGI_WASI_BIN_DIR_NAME)
    }

    fn ipc_dir(&self) -> PathBuf {
        let dir = self.fungi_dir().join(DEFAULT_IPC_DIR_NAME);
        if !dir.exists() {
            std::fs::create_dir(&dir).unwrap();
        }
        dir
    }

    fn mush_ipc_path(&self) -> PathBuf {
        self.ipc_dir()
            .join(MUSH_LISTENER_ADDR)
    }
}

impl FungiDir for DaemonArgs {
    fn fungi_dir(&self) -> PathBuf {
        self.fungi_dir
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| home::home_dir().unwrap().join(DEFAULT_FUNGI_DIR))
    }
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
