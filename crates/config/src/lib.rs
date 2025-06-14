pub mod file_transfer;
mod fra;
mod init;
mod libp2p;
mod tcp_tunneling;

pub use init::init;

use fra::*;
use libp2p::*;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tcp_tunneling::*;

use crate::file_transfer::FileTransfer;

pub const DEFAULT_CONFIG_FILE: &str = "config.toml";
pub const DEFAULT_FUNGI_DIR: &str = ".fungi";
pub const DEFAULT_IPC_DIR_NAME: &str = ".ipc";
pub const DEFAULT_DAEMON_RPC_NAME: &str = ".fungi_daemon.sock";
pub const FRA_LISTENER_ADDR: &str = ".fungi_ra.sock"; // FRA: Fungi Remote Access

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct FungiConfig {
    #[serde(default)]
    pub tcp_tunneling: TcpTunneling,
    #[serde(default)]
    pub libp2p: Libp2p,
    #[serde(default)]
    pub fungi_remote_access: FungiRemoteAccess,
    #[serde(default)]
    pub file_transfer: FileTransfer,
}

impl FungiConfig {
    pub fn apply_from_dir(fungi_dir: &Path) -> Result<Self, toml::de::Error> {
        let s = std::fs::read_to_string(fungi_dir.join(DEFAULT_CONFIG_FILE))
            .expect("Failed to read config file");
        let cfg = Self::parse_toml(&s)?;
        Ok(cfg)
    }

    pub fn set_fra_allow_all_peers(&mut self, allow: bool) {
        self.fungi_remote_access.allow_all_peers = allow;
    }

    pub fn parse_toml(s: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(s)
    }
}

pub trait FungiDir {
    fn fungi_dir(&self) -> PathBuf;

    fn ipc_dir(&self) -> PathBuf {
        let dir = self.fungi_dir().join(DEFAULT_IPC_DIR_NAME);
        if !dir.exists() {
            std::fs::create_dir(&dir).unwrap();
        }
        dir
    }

    fn daemon_rpc_path(&self) -> PathBuf {
        self.ipc_dir().join(DEFAULT_DAEMON_RPC_NAME)
    }

    // FRA: Fungi Remote Access
    fn fra_ipc_path(&self) -> PathBuf {
        self.ipc_dir().join(FRA_LISTENER_ADDR)
    }
}
