use libp2p_identity::PeerId;
use multiaddr::Multiaddr;
use serde::{Deserialize, Serialize};

const fn default_idle_connection_timeout_secs() -> u64 {
    300
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Network {
    #[serde(default)]
    pub listen_tcp_port: u16,
    #[serde(default)]
    pub listen_udp_port: u16,
    #[serde(default)]
    pub incoming_allowed_peers: Vec<PeerId>,
    #[serde(default)]
    pub disable_relay: bool,
    #[serde(default)]
    pub custom_relay_addresses: Vec<Multiaddr>,
    #[serde(default = "default_idle_connection_timeout_secs")]
    pub idle_connection_timeout_secs: u64,
}

impl Default for Network {
    fn default() -> Self {
        Self {
            listen_tcp_port: 0,
            listen_udp_port: 0,
            incoming_allowed_peers: Vec::new(),
            disable_relay: false,
            custom_relay_addresses: Vec::new(),
            idle_connection_timeout_secs: default_idle_connection_timeout_secs(),
        }
    }
}
