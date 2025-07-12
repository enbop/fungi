use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerHandshakePayload {
    host_name: Option<String>,
}

impl PeerHandshakePayload {
    pub fn new() -> Self {
        Self {
            host_name: fungi_util::sysinfo::System::host_name(),
        }
    }

    pub fn host_name(&self) -> Option<String> {
        self.host_name.clone()
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(bytes)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        bincode::serialize(self).unwrap()
    }
}
