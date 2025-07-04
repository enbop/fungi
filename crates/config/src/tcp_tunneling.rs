use std::net::{AddrParseError, SocketAddr};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct LocalSocket {
    pub host: String,
    pub port: u16,
}

impl TryInto<SocketAddr> for &LocalSocket {
    type Error = AddrParseError;

    fn try_into(self) -> Result<SocketAddr, Self::Error> {
        format!("{}:{}", self.host, self.port).parse()
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ForwardingRuleRemote {
    pub peer_id: String,
    pub port: u16,
}

// [[tcp-tunneling.forwarding.rules]]
// local_socket = { host = "127.0.0.1", port = 9001 }
// remote = { peer_id = "", port = 8888, multiaddrs = [] }
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ForwardingRule {
    pub local_socket: LocalSocket,
    pub remote: ForwardingRuleRemote,
}

// [[tcp-tunneling.listening.rules]]
// local_socket = { host = "127.0.0.1", port = 9002 }
// listening_port = 8888
// allowed_peers = []
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ListeningRule {
    pub local_socket: LocalSocket,
    pub listening_port: u16,
    #[serde(default)]
    pub allowed_peers: Vec<String>,
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
