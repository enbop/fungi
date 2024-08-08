mod libp2p;
mod mushd;
mod tcp_tunneling;

use libp2p::*;
use mushd::*;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tcp_tunneling::*;

use crate::{
    commands::{Commands, FungiArgs},
    DEFAULT_CONFIG_FILE,
};

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
    pub fn apply_from_args(args: &FungiArgs) -> Result<Self, toml::de::Error> {
        let s = std::fs::read_to_string(args.fungi_dir().join(DEFAULT_CONFIG_FILE))
            .expect("Failed to read config file");
        let mut cfg = Self::parse_toml(&s)?;

        // debug allow all inbound peers
        if let Some(Commands::Daemon {
            debug_allow_all_peers: Some(allow),
        }) = &args.command
        {
            cfg.mush_daemon.allow_all_peers = *allow;
        }

        Ok(cfg)
    }

    pub fn parse_toml(s: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(s)
    }
}
