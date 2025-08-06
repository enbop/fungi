use anyhow::Result;
use libp2p_identity::PeerId;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

pub const DEFAULT_PEERS_CONFIG_FILE: &str = "known_peers.toml";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PeerInfo {
    pub peer_id: PeerId,
    pub hostname: Option<String>,
    pub public_ip: Option<String>,
    pub private_ips: Vec<String>,
    pub created_at: u64,
    pub last_connected: Option<u64>,
}

impl PeerInfo {
    pub fn new(peer_id: PeerId, hostname: Option<String>) -> Self {
        let created_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            peer_id,
            hostname,
            public_ip: None,
            private_ips: Vec::new(),
            created_at,
            last_connected: None,
        }
    }

    pub fn peer_id(&self) -> Result<PeerId, String> {
        Ok(self.peer_id)
    }

    pub fn update_last_connected(&mut self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.last_connected = Some(now);
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct KnownPeersConfig {
    #[serde(default)]
    pub peers: Vec<PeerInfo>,

    #[serde(skip)]
    config_file: PathBuf,
}

impl KnownPeersConfig {
    pub fn config_file_path(&self) -> &Path {
        &self.config_file
    }

    pub fn init_config_file(config_file: PathBuf) -> Result<()> {
        if config_file.exists() {
            return Ok(());
        }
        let config = KnownPeersConfig::default();
        let toml_string = toml::to_string(&config)?;
        std::fs::write(&config_file, toml_string)?;
        Ok(())
    }

    pub fn apply_from_dir(fungi_dir: &Path) -> Result<Self> {
        let config_file = fungi_dir.join(DEFAULT_PEERS_CONFIG_FILE);
        if !config_file.exists() {
            Self::init_config_file(config_file.clone())?;
        }

        let s = std::fs::read_to_string(&config_file)?;
        let mut cfg = Self::parse_toml(&s)?;
        cfg.config_file = config_file;
        Ok(cfg)
    }

    pub fn parse_toml(s: &str) -> Result<Self> {
        let config: Self = toml::from_str(s)?;
        Ok(config)
    }

    pub fn save_to_file(&self) -> Result<()> {
        let toml_string = toml::to_string_pretty(self)?;
        std::fs::write(&self.config_file, toml_string)?;
        Ok(())
    }

    fn update_and_save<F>(&self, updater: F) -> Result<Self>
    where
        F: FnOnce(&mut Self),
    {
        let mut new_config = self.clone();
        updater(&mut new_config);
        new_config.save_to_file()?;
        Ok(new_config)
    }

    pub fn add_or_update_peer(&self, peer_id: PeerId, hostname: Option<String>) -> Result<Self> {
        self.update_and_save(|config| {
            if let Some(existing_peer) = config.peers.iter_mut().find(|p| p.peer_id == peer_id) {
                // Update existing peer
                if hostname.is_some() && existing_peer.hostname != hostname {
                    existing_peer.hostname = hostname;
                }
                existing_peer.update_last_connected();
            } else {
                // Add new peer
                let peer_info = PeerInfo::new(peer_id, hostname);
                config.peers.push(peer_info);
            }
        })
    }

    pub fn get_peer_info(&self, peer_id: &PeerId) -> Option<&PeerInfo> {
        self.peers.iter().find(|p| p.peer_id == *peer_id)
    }

    pub fn get_all_peers(&self) -> &Vec<PeerInfo> {
        &self.peers
    }

    pub fn get_peers_map(&self) -> HashMap<PeerId, PeerInfo> {
        self.peers
            .iter()
            .map(|peer| (peer.peer_id, peer.clone()))
            .collect()
    }

    pub fn remove_peer(&self, peer_id: &PeerId) -> Result<Self> {
        self.update_and_save(|config| {
            config.peers.retain(|p| p.peer_id != *peer_id);
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_temp_peers_config() -> (KnownPeersConfig, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join("fungi-test");
        std::fs::create_dir_all(&config_dir).unwrap();
        (
            KnownPeersConfig::apply_from_dir(&config_dir).unwrap(),
            temp_dir,
        )
    }

    #[test]
    fn test_init_peers_config_file() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join(DEFAULT_PEERS_CONFIG_FILE);
        KnownPeersConfig::init_config_file(config_path.clone()).unwrap();
        assert!(config_path.exists());
        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("peers = []"));
    }

    #[test]
    fn test_add_peer() {
        let (config, _temp_dir) = create_temp_peers_config();
        let peer_id = PeerId::random();
        let hostname = Some("test-host".to_string());

        let updated_config = config
            .add_or_update_peer(peer_id, hostname.clone())
            .unwrap();

        let peer_info = updated_config.get_peer_info(&peer_id).unwrap();
        assert_eq!(peer_info.peer_id, peer_id);
        assert_eq!(peer_info.hostname, hostname);
    }

    #[test]
    fn test_update_existing_peer() {
        let (config, _temp_dir) = create_temp_peers_config();
        let peer_id = PeerId::random();

        // Add peer first
        let config = config
            .add_or_update_peer(peer_id, Some("host1".to_string()))
            .unwrap();

        // Update with new hostname
        let updated_config = config
            .add_or_update_peer(peer_id, Some("host2".to_string()))
            .unwrap();

        let peer_info = updated_config.get_peer_info(&peer_id).unwrap();
        assert_eq!(peer_info.hostname, Some("host2".to_string()));
        assert!(peer_info.last_connected.is_some());
    }
}
