mod daemon;
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

    #[arg(short, long)]
    pub fungi_dir: Option<String>,

    #[arg(long)]
    pub wasi_root_dir: Option<String>,

    #[arg(long)]
    pub wasi_bin_dir: Option<String>,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Initialize Fungi
    Init,

    /// Start a Fungi daemon
    Daemon {
        #[arg(long)]
        debug_allow_all_peers: Option<bool>,
    },

    /// Start a Fungi wasi process
    Wasi,

    Mush{
        /// Connect to a remote peer
        #[arg(short, long)]
        peer: Option<PeerId>,
    },
}

impl FungiArgs {
    pub fn fungi_dir(&self) -> PathBuf {
        self.fungi_dir
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| home::home_dir().unwrap().join(DEFAULT_FUNGI_DIR))
    }

    pub fn wasi_root_dir(&self) -> PathBuf {
        self.wasi_root_dir
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| self.fungi_dir().join(DEFAULT_FUNGI_WASI_ROOT_DIR_NAME))
    }

    pub fn wasi_bin_dir(&self) -> PathBuf {
        self.wasi_bin_dir
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| self.wasi_root_dir().join(DEFAULT_FUNGI_WASI_BIN_DIR_NAME))
    }

    pub fn ipc_dir(&self) -> PathBuf {
        let dir = self.fungi_dir().join(DEFAULT_IPC_DIR_NAME);
        if !dir.exists() {
            std::fs::create_dir(&dir).unwrap();
        }
        dir
    }

    pub fn mush_ipc_path(&self) -> PathBuf {
        self.ipc_dir().join(MUSH_LISTENER_ADDR)
    }
}
