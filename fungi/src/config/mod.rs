mod libp2p;
mod mushd;
mod tcp_tunneling;

use libp2p::*;
use mushd::*;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tcp_tunneling::*;

use crate::DEFAULT_CONFIG_FILE;

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
