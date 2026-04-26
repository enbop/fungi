use anyhow::Result;
use libp2p_identity::PeerId;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
    time::SystemTime,
};

pub const DEFAULT_ADDRESS_BOOK_CONFIG_FILE: &str = "address_book.toml";
const MDNS_DEVICE_TIMEOUT_SECONDS: u64 = 3600; // 60 minutes

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum Os {
    Windows,
    MacOS,
    Linux,
    Android,
    IOS,
    Unknown,
}

impl Os {
    pub fn this_device() -> Self {
        if cfg!(target_os = "windows") {
            Os::Windows
        } else if cfg!(target_os = "macos") {
            Os::MacOS
        } else if cfg!(target_os = "linux") {
            Os::Linux
        } else if cfg!(target_os = "android") {
            Os::Android
        } else if cfg!(target_os = "ios") {
            Os::IOS
        } else {
            Os::Unknown
        }
    }
}

impl From<&Os> for String {
    fn from(val: &Os) -> Self {
        match val {
            Os::Windows => "Windows".to_string(),
            Os::MacOS => "MacOS".to_string(),
            Os::Linux => "Linux".to_string(),
            Os::Android => "Android".to_string(),
            Os::IOS => "iOS".to_string(),
            Os::Unknown => "Unknown".to_string(),
        }
    }
}

impl From<&str> for Os {
    fn from(value: &str) -> Self {
        match value {
            "Windows" => Os::Windows,
            "MacOS" => Os::MacOS,
            "Linux" => Os::Linux,
            "Android" => Os::Android,
            "iOS" => Os::IOS,
            _ => Os::Unknown,
        }
    }
}

// use this for both mdns and address_book
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PeerInfo {
    // set on local device
    pub peer_id: PeerId,
    pub alias: Option<String>,
    pub hostname: Option<String>,
    #[serde(default)]
    pub multiaddrs: Vec<String>,
    pub private_ips: Vec<String>,
    pub os: Os,
    pub version: String,

    // set on remote devices
    pub public_ip: Option<String>,
    pub created_at: SystemTime,
    pub last_connected: SystemTime,
}

impl PeerInfo {
    pub fn this_device(peer_id: PeerId, hostname: Option<String>) -> Self {
        let version = std::env!("CARGO_PKG_VERSION").to_string();
        let local_ip = fungi_util::get_local_ip();
        let os = Os::this_device();
        Self {
            peer_id,
            alias: None,
            hostname,
            multiaddrs: vec![],
            os,
            public_ip: None,
            private_ips: local_ip.map(|ip| vec![ip]).unwrap_or_default(),
            version,
            created_at: SystemTime::now(),
            last_connected: SystemTime::now(),
        }
    }

    pub fn update_from(&mut self, other: Self) {
        let old_created_at = self.created_at;
        let old_alias = self.alias.clone();
        *self = other;

        // Preserve original alias if not set in the new info
        if self.alias.is_none() {
            self.alias = old_alias;
        }

        // Preserve original creation time
        self.created_at = old_created_at;
        self.update_last_connected();
    }

    pub fn update_last_connected(&mut self) {
        self.last_connected = SystemTime::now();
    }

    pub fn is_expired(&self) -> bool {
        if let Ok(elapsed) = self.created_at.elapsed() {
            elapsed.as_secs() > MDNS_DEVICE_TIMEOUT_SECONDS
        } else {
            true // If we can't determine elapsed time, consider it expired
        }
    }

    pub fn new_unknown(peer_id: PeerId) -> Self {
        Self {
            peer_id,
            alias: None,
            hostname: None,
            multiaddrs: vec![],
            os: Os::Unknown,
            public_ip: None,
            private_ips: vec![],
            version: String::new(),
            created_at: SystemTime::now(),
            last_connected: SystemTime::now(),
        }
    }
}

impl TryFrom<&mdns_sd::TxtProperties> for PeerInfo {
    type Error = io::Error;

    fn try_from(properties: &mdns_sd::TxtProperties) -> std::result::Result<Self, Self::Error> {
        let peer_id_str = properties
            .get("peer_id")
            .ok_or(io::Error::other("Missing peer_id property"))?
            .val_str();
        let peer_id = peer_id_str
            .parse::<PeerId>()
            .map_err(|_| io::Error::other(format!("Invalid peer_id: {}", peer_id_str)))?;

        let hostname = properties.get("hostname").map(|s| s.val_str().to_string());
        let os = properties
            .get("os")
            .map(|s| Os::from(s.val_str()))
            .unwrap_or(Os::Unknown);
        let version = properties
            .get("version")
            .map(|s| s.val_str().to_string())
            .unwrap_or_default();
        let public_ip = properties.get("public_ip").map(|s| s.val_str().to_string());

        let private_ips = properties
            .get("private_ips")
            .map(|s| s.val_str().split(',').map(String::from).collect())
            .unwrap_or_default();
        // TODO duplicate with libp2p-mdns?
        let multiaddrs = properties
            .get("multiaddrs")
            .map(|s| {
                s.val_str()
                    .split(',')
                    .filter(|value| !value.is_empty())
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default();

        Ok(PeerInfo {
            peer_id,
            alias: None,
            hostname,
            multiaddrs,
            os,
            public_ip,
            private_ips,
            version,
            created_at: SystemTime::now(),
            last_connected: SystemTime::now(),
        })
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AddressBookConfig {
    #[serde(default)]
    pub peers: Vec<PeerInfo>,

    #[serde(skip)]
    config_file: PathBuf,
}

impl AddressBookConfig {
    pub fn config_file_path(&self) -> &Path {
        &self.config_file
    }

    pub fn init_config_file(config_file: PathBuf) -> Result<()> {
        if config_file.exists() {
            return Ok(());
        }
        let config = AddressBookConfig::default();
        let toml_string = toml::to_string(&config)?;
        std::fs::write(&config_file, toml_string)?;
        Ok(())
    }

    pub fn apply_from_dir(fungi_dir: &Path) -> Result<Self> {
        let config_file = fungi_dir.join(DEFAULT_ADDRESS_BOOK_CONFIG_FILE);
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

    pub fn normalize_alias(alias: &str) -> String {
        alias.trim().to_lowercase()
    }

    pub fn validate_alias(alias: &str) -> Result<String> {
        let trimmed = alias.trim();
        if trimmed.is_empty() {
            return Err(anyhow::anyhow!("Alias cannot be empty"));
        }
        Ok(trimmed.to_string())
    }

    pub fn alias_exists(&self, alias: &str, except_peer_id: Option<&PeerId>) -> bool {
        let normalized = Self::normalize_alias(alias);
        self.peers.iter().any(|peer| {
            if let Some(except_peer_id) = except_peer_id
                && peer.peer_id == *except_peer_id
            {
                return false;
            }

            peer.alias.as_deref().map(Self::normalize_alias).as_deref() == Some(normalized.as_str())
        })
    }

    pub fn get_peer_by_alias(&self, alias: &str) -> Option<&PeerInfo> {
        let normalized = Self::normalize_alias(alias);
        self.peers.iter().find(|peer| {
            peer.alias.as_deref().map(Self::normalize_alias).as_deref() == Some(normalized.as_str())
        })
    }

    pub fn add_or_update_peer(&self, peer_info: PeerInfo) -> Result<Self> {
        let validated_alias = match peer_info.alias.as_deref() {
            Some(alias) => Self::validate_alias(alias)?,
            None => {
                return Err(anyhow::anyhow!(
                    "Device name is required for managed devices"
                ));
            }
        };

        if self.alias_exists(&validated_alias, Some(&peer_info.peer_id)) {
            return Err(anyhow::anyhow!(
                "Device name already exists: {}",
                validated_alias
            ));
        }

        self.update_and_save(|config| {
            let mut peer_info = peer_info.clone();
            peer_info.alias = Some(validated_alias.clone());
            if let Some(existing_peer) = config
                .peers
                .iter_mut()
                .find(|p| p.peer_id == peer_info.peer_id)
            {
                // Update existing peer
                existing_peer.update_from(peer_info.clone());
            } else {
                // Add new peer
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

    pub fn sync_peer_multiaddrs(&self, updates: HashMap<PeerId, Vec<String>>) -> Result<Self> {
        let mut new_config = self.clone();
        let mut changed = false;

        for (peer_id, multiaddrs) in updates {
            let mut normalized = multiaddrs
                .into_iter()
                .map(|addr| addr.trim().to_string())
                .filter(|addr| !addr.is_empty())
                .collect::<Vec<_>>();
            normalized.sort();
            normalized.dedup();

            if normalized.is_empty() {
                continue;
            }

            if let Some(existing_peer) = new_config.peers.iter_mut().find(|p| p.peer_id == peer_id)
            {
                if existing_peer.multiaddrs != normalized {
                    existing_peer.multiaddrs = normalized;
                    changed = true;
                }
                continue;
            }

            // Learned addresses are connection cache, not user-managed devices.
            // Unknown peers must not appear in the address book unless the user adds them.
            continue;
        }

        if changed {
            new_config.save_to_file()?;
        }

        Ok(new_config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_temp_peers_config() -> (AddressBookConfig, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join("fungi-test");
        std::fs::create_dir_all(&config_dir).unwrap();
        (
            AddressBookConfig::apply_from_dir(&config_dir).unwrap(),
            temp_dir,
        )
    }

    #[test]
    fn test_init_peers_config_file() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join(DEFAULT_ADDRESS_BOOK_CONFIG_FILE);
        AddressBookConfig::init_config_file(config_path.clone()).unwrap();
        assert!(config_path.exists());
        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("peers = []"));
    }

    #[test]
    fn test_add_peer() {
        let (config, _temp_dir) = create_temp_peers_config();
        let peer_id = PeerId::random();
        let hostname = Some("test-host".to_string());
        let peer_info = PeerInfo {
            peer_id,
            alias: Some("test-device".to_string()),
            hostname: hostname.clone(),
            multiaddrs: vec![],
            os: Os::this_device(),
            public_ip: None,
            private_ips: vec![],
            version: "1.0.0".to_string(),
            created_at: SystemTime::now(),
            last_connected: SystemTime::now(),
        };

        let updated_config = config.add_or_update_peer(peer_info).unwrap();

        let peer_info = updated_config.get_peer_info(&peer_id).unwrap();
        assert_eq!(peer_info.peer_id, peer_id);
        assert_eq!(peer_info.alias.as_deref(), Some("test-device"));
        assert_eq!(peer_info.hostname, hostname);
    }

    #[test]
    fn test_update_existing_peer() {
        let (config, _temp_dir) = create_temp_peers_config();
        let peer_id = PeerId::random();
        let initial_peer_info = PeerInfo {
            peer_id,
            alias: Some("device1".to_string()),
            hostname: Some("host1".to_string()),
            multiaddrs: vec![],
            os: Os::this_device(),
            public_ip: None,
            private_ips: vec![],
            version: "1.0.0".to_string(),
            created_at: SystemTime::now(),
            last_connected: SystemTime::now(),
        };

        let updated_config = config.add_or_update_peer(initial_peer_info).unwrap();
        let updated_peer_info = PeerInfo {
            peer_id,
            alias: Some("alias1".to_string()),
            hostname: Some("host2".to_string()),
            multiaddrs: vec![],
            os: Os::this_device(),
            public_ip: None,
            private_ips: vec![],
            version: "1.0.1".to_string(),
            created_at: SystemTime::now(),
            last_connected: SystemTime::now(),
        };
        let updated_config = updated_config
            .add_or_update_peer(updated_peer_info)
            .unwrap();
        let peer_info = updated_config.get_peer_info(&peer_id).unwrap();
        assert_eq!(peer_info.alias, Some("alias1".to_string()));
        assert_eq!(peer_info.hostname, Some("host2".to_string()));
        assert_eq!(peer_info.version, "1.0.1");
    }

    #[test]
    fn test_add_peer_rejects_duplicate_alias_case_insensitive() {
        let (config, _temp_dir) = create_temp_peers_config();

        let first = PeerInfo {
            peer_id: PeerId::random(),
            alias: Some("MacBook".to_string()),
            hostname: None,
            multiaddrs: vec![],
            os: Os::this_device(),
            public_ip: None,
            private_ips: vec![],
            version: "1.0.0".to_string(),
            created_at: SystemTime::now(),
            last_connected: SystemTime::now(),
        };

        let second = PeerInfo {
            peer_id: PeerId::random(),
            alias: Some("  macbook  ".to_string()),
            hostname: None,
            multiaddrs: vec![],
            os: Os::this_device(),
            public_ip: None,
            private_ips: vec![],
            version: "1.0.0".to_string(),
            created_at: SystemTime::now(),
            last_connected: SystemTime::now(),
        };

        let updated = config.add_or_update_peer(first).unwrap();
        assert!(updated.add_or_update_peer(second).is_err());
    }

    #[test]
    fn test_add_peer_rejects_missing_alias() {
        let (config, _temp_dir) = create_temp_peers_config();
        let peer_info = PeerInfo {
            peer_id: PeerId::random(),
            alias: None,
            hostname: Some("host1".to_string()),
            multiaddrs: vec![],
            os: Os::this_device(),
            public_ip: None,
            private_ips: vec![],
            version: "1.0.0".to_string(),
            created_at: SystemTime::now(),
            last_connected: SystemTime::now(),
        };

        assert!(config.add_or_update_peer(peer_info).is_err());
    }

    #[test]
    fn test_add_peer_trims_alias_before_saving() {
        let (config, _temp_dir) = create_temp_peers_config();
        let peer_id = PeerId::random();
        let peer_info = PeerInfo {
            peer_id,
            alias: Some("  work-laptop  ".to_string()),
            hostname: None,
            multiaddrs: vec![],
            os: Os::this_device(),
            public_ip: None,
            private_ips: vec![],
            version: "1.0.0".to_string(),
            created_at: SystemTime::now(),
            last_connected: SystemTime::now(),
        };

        let updated = config.add_or_update_peer(peer_info).unwrap();
        assert_eq!(
            updated
                .get_peer_info(&peer_id)
                .and_then(|peer| peer.alias.clone()),
            Some("work-laptop".to_string())
        );
    }

    #[test]
    fn test_sync_peer_multiaddrs_updates_existing_peer() {
        let (config, _temp_dir) = create_temp_peers_config();
        let peer_id = PeerId::random();
        let updated = config
            .add_or_update_peer(PeerInfo {
                peer_id,
                alias: Some("host1".to_string()),
                hostname: Some("host1".to_string()),
                multiaddrs: vec![],
                os: Os::this_device(),
                public_ip: None,
                private_ips: vec![],
                version: "1.0.0".to_string(),
                created_at: SystemTime::now(),
                last_connected: SystemTime::now(),
            })
            .unwrap();

        let synced = updated
            .sync_peer_multiaddrs(HashMap::from([(
                peer_id,
                vec![
                    "/ip4/192.168.1.7/tcp/4001".to_string(),
                    "/ip4/192.168.1.7/tcp/4001".to_string(),
                ],
            )]))
            .unwrap();

        let peer = synced.get_peer_info(&peer_id).unwrap();
        assert_eq!(
            peer.multiaddrs,
            vec!["/ip4/192.168.1.7/tcp/4001".to_string()]
        );
    }

    #[test]
    fn test_sync_peer_multiaddrs_ignores_unknown_peer() {
        let (config, _temp_dir) = create_temp_peers_config();
        let peer_id = PeerId::random();

        let synced = config
            .sync_peer_multiaddrs(HashMap::from([(
                peer_id,
                vec!["/ip4/192.168.1.7/tcp/4001".to_string()],
            )]))
            .unwrap();

        assert!(synced.get_peer_info(&peer_id).is_none());
        assert!(synced.get_all_peers().is_empty());
    }
}
