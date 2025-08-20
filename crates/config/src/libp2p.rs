use libp2p_identity::PeerId;
use multiaddr::Multiaddr;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
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
}
