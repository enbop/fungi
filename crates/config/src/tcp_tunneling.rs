use std::net::{AddrParseError, SocketAddr};

use multiaddr::Multiaddr;
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
    pub protocol: String,
    #[serde(default)]
    pub multiaddrs: Vec<Multiaddr>,
}

// [[tcp-tunneling.forwarding.rules]]
// local_socket = { host = "127.0.0.1", port = 9001 }
// remote = { peer_id = "", protocol = "", multiaddrs = [] }
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ForwardingRule {
    pub local_socket: LocalSocket,
    pub remote: ForwardingRuleRemote,
}

// [[tcp-tunneling.listening.rules]]
// local_socket = { host = "127.0.0.1", port = 9002 }
// listening_protocol = ""
// allowed_peers = []
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ListeningRule {
    pub local_socket: LocalSocket,
    pub listening_protocol: String,
    #[serde(default)]
    pub allowed_peers: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Forwarding {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub rules: Vec<ForwardingRule>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Listening {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub rules: Vec<ListeningRule>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct TcpTunneling {
    #[serde(default)]
    pub forwarding: Forwarding,
    #[serde(default)]
    pub listening: Listening,
}
