use std::time::SystemTime;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ServiceCache {
    #[serde(default)]
    pub peers: Vec<CachedPeerServices>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CachedPeerServices {
    pub peer_id: String,
    pub services_json: String,
    pub updated_at: SystemTime,
}

impl ServiceCache {
    pub fn get_peer_services_json(&self, peer_id: &str) -> Option<&str> {
        self.peers
            .iter()
            .find(|entry| entry.peer_id == peer_id)
            .map(|entry| entry.services_json.as_str())
    }

    pub fn set_peer_services_json(&mut self, peer_id: String, services_json: String) {
        if let Some(entry) = self.peers.iter_mut().find(|entry| entry.peer_id == peer_id) {
            entry.services_json = services_json;
            entry.updated_at = SystemTime::now();
            return;
        }

        self.peers.push(CachedPeerServices {
            peer_id,
            services_json,
            updated_at: SystemTime::now(),
        });
        self.peers
            .sort_by(|left, right| left.peer_id.cmp(&right.peer_id));
    }
}
