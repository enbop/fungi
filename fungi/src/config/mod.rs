mod tcp_tunneling;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tcp_tunneling::*;

use crate::DEFAULT_CONFIG_FILE;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct FungiConfig {
    #[serde(default)]
    pub tcp_tunneling: TcpTunneling,
}

impl FungiConfig {
    pub fn parse_from_dir(fungi_dir: &PathBuf) -> Result<Self, toml::de::Error> {
        let s = std::fs::read_to_string(fungi_dir.join(DEFAULT_CONFIG_FILE))
            .expect("Failed to read config file");
        Self::parse_toml(&s)
    }

    pub fn parse_toml(s: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(s)
    }
}
