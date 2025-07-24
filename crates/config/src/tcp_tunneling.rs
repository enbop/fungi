use std::net::{AddrParseError, SocketAddr};

use serde::{Deserialize, Serialize};

impl TryInto<SocketAddr> for &ListeningRule {
    type Error = AddrParseError;

    fn try_into(self) -> Result<SocketAddr, Self::Error> {
        format!("{}:{}", self.host, self.port).parse()
    }
}

impl TryInto<SocketAddr> for &ForwardingRule {
    type Error = AddrParseError;

    fn try_into(self) -> Result<SocketAddr, Self::Error> {
        format!("{}:{}", self.local_host, self.local_port).parse()
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ForwardingRule {
    pub local_host: String,
    pub local_port: u16,

    pub remote_peer_id: String,
    pub remote_port: u16,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ListeningRule {
    pub host: String,
    pub port: u16,
    // #[serde(default)]
    // pub allowed_peers: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Forwarding {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub rules: Vec<ForwardingRule>,
}

impl Default for Forwarding {
    fn default() -> Self {
        Self {
            enabled: true,
            rules: Default::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Listening {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub rules: Vec<ListeningRule>,
}

impl Default for Listening {
    fn default() -> Self {
        Self {
            enabled: true,
            rules: Default::default(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct TcpTunneling {
    #[serde(default)]
    pub forwarding: Forwarding,
    #[serde(default)]
    pub listening: Listening,
}
