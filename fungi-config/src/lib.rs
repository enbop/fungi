mod libp2p;
mod mushd;
mod tcp_tunneling;
mod init;

pub use init::init;

use libp2p::*;
use mushd::*;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tcp_tunneling::*;

pub const DEFAULT_CONFIG_FILE: &str = "config.toml";
pub const DEFAULT_FUNGI_DIR: &str = ".fungi";
pub const DEFAULT_FUNGI_WASI_ROOT_DIR_NAME: &str = "root";
pub const DEFAULT_FUNGI_WASI_BIN_DIR_NAME: &str = "bin";
pub const DEFAULT_IPC_DIR_NAME: &str = ".ipc";
pub const MUSH_LISTENER_ADDR: &str = ".fungi_mush.sock";

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct FungiConfig {
    #[serde(default)]
    pub tcp_tunneling: TcpTunneling,
    #[serde(default)]
    pub libp2p: Libp2p,
    #[serde(default)]
    pub mush_daemon: MushDaemon,
}

impl FungiConfig {
    pub fn apply_from_dir(fungi_dir: &Path) -> Result<Self, toml::de::Error> {
        let s = std::fs::read_to_string(fungi_dir.join(DEFAULT_CONFIG_FILE))
            .expect("Failed to read config file");
        let cfg = Self::parse_toml(&s)?;
        Ok(cfg)
    }

    pub fn set_mush_daemon_allow_all_peers(&mut self, allow: bool) {
        self.mush_daemon.allow_all_peers = allow;
    }

    pub fn parse_toml(s: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(s)
    }
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
        self.ipc_dir().join(MUSH_LISTENER_ADDR)
    }
}
