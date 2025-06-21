use libp2p_identity::PeerId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Libp2p {
    #[serde(default)]
    pub listen_tcp_port: u16,
    #[serde(default)]
    pub listen_udp_port: u16,
    #[serde(default)]
    pub incoming_allowed_peers: Vec<PeerId>,
}
