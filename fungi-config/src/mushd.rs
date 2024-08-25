use libp2p_identity::PeerId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct MushDaemon {
    #[serde(default)]
    pub enable: bool,
    #[serde(default)]
    pub allow_peers: Vec<PeerId>,
    #[serde(default)]
    pub allow_all_peers: bool,
}
