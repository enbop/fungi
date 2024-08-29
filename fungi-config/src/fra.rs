use libp2p_identity::PeerId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct FungiRemoteAccess {
    #[serde(default)]
    pub enable: bool,
    #[serde(default)]
    pub allowed_peers: Vec<PeerId>,
    #[serde(default)]
    pub allow_all_peers: bool,
}
