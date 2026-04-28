use anyhow::Result;
use libp2p_identity::PeerId;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
    time::SystemTime,
};

pub const DEFAULT_DEVICES_CONFIG_FILE: &str = "devices.toml";
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

// use this for both mdns and devices
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DeviceInfo {
    // set on local device
    pub peer_id: PeerId,
    pub name: Option<String>,
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

impl DeviceInfo {
    pub fn this_device(peer_id: PeerId, hostname: Option<String>) -> Self {
        let version = std::env!("CARGO_PKG_VERSION").to_string();
        let local_ip = fungi_util::get_local_ip();
        let os = Os::this_device();
        Self {
            peer_id,
            name: None,
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
        let old_name = self.name.clone();
        *self = other;

        // Preserve original name if not set in the new info
        if self.name.is_none() {
            self.name = old_name;
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
            name: None,
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

impl TryFrom<&mdns_sd::TxtProperties> for DeviceInfo {
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

        Ok(DeviceInfo {
            peer_id,
            name: None,
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
pub struct DevicesConfig {
    #[serde(default)]
    pub devices: Vec<DeviceInfo>,

    #[serde(skip)]
    config_file: PathBuf,
}

impl DevicesConfig {
    pub fn config_file_path(&self) -> &Path {
        &self.config_file
    }

    pub fn init_config_file(config_file: PathBuf) -> Result<()> {
        if config_file.exists() {
            return Ok(());
        }
        let config = DevicesConfig::default();
        let toml_string = toml::to_string(&config)?;
        std::fs::write(&config_file, toml_string)?;
        Ok(())
    }

    pub fn apply_from_dir(fungi_dir: &Path) -> Result<Self> {
        let config_file = fungi_dir.join(DEFAULT_DEVICES_CONFIG_FILE);
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

    pub fn normalize_name(name: &str) -> String {
        name.trim().to_lowercase()
    }

    pub fn validate_name(name: &str) -> Result<String> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(anyhow::anyhow!("Name cannot be empty"));
        }
        Ok(trimmed.to_string())
    }

    pub fn name_exists(&self, name: &str, except_peer_id: Option<&PeerId>) -> bool {
        let normalized = Self::normalize_name(name);
        self.devices.iter().any(|peer| {
            if let Some(except_peer_id) = except_peer_id
                && peer.peer_id == *except_peer_id
            {
                return false;
            }

            peer.name.as_deref().map(Self::normalize_name).as_deref() == Some(normalized.as_str())
        })
    }

    pub fn get_device_by_name(&self, name: &str) -> Option<&DeviceInfo> {
        let normalized = Self::normalize_name(name);
        self.devices.iter().find(|device| {
            device.name.as_deref().map(Self::normalize_name).as_deref() == Some(normalized.as_str())
        })
    }

    pub fn add_or_update_device(&self, device_info: DeviceInfo) -> Result<Self> {
        let validated_name = match device_info.name.as_deref() {
            Some(name) => Self::validate_name(name)?,
            None => {
                return Err(anyhow::anyhow!(
                    "Device name is required for managed devices"
                ));
            }
        };

        if self.name_exists(&validated_name, Some(&device_info.peer_id)) {
            return Err(anyhow::anyhow!(
                "Device name already exists: {}",
                validated_name
            ));
        }

        self.update_and_save(|config| {
            let mut device_info = device_info.clone();
            device_info.name = Some(validated_name.clone());
            if let Some(existing_device) = config
                .devices
                .iter_mut()
                .find(|p| p.peer_id == device_info.peer_id)
            {
                existing_device.update_from(device_info.clone());
            } else {
                config.devices.push(device_info);
            }
        })
    }

    pub fn get_device_info(&self, peer_id: &PeerId) -> Option<&DeviceInfo> {
        self.devices.iter().find(|p| p.peer_id == *peer_id)
    }

    pub fn get_all_devices(&self) -> &Vec<DeviceInfo> {
        &self.devices
    }

    pub fn get_devices_map(&self) -> HashMap<PeerId, DeviceInfo> {
        self.devices
            .iter()
            .map(|device| (device.peer_id, device.clone()))
            .collect()
    }

    pub fn remove_device(&self, peer_id: &PeerId) -> Result<Self> {
        self.update_and_save(|config| {
            config.devices.retain(|p| p.peer_id != *peer_id);
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_temp_devices_config() -> (DevicesConfig, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join("fungi-test");
        std::fs::create_dir_all(&config_dir).unwrap();
        (
            DevicesConfig::apply_from_dir(&config_dir).unwrap(),
            temp_dir,
        )
    }

    #[test]
    fn test_init_devices_config_file() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join(DEFAULT_DEVICES_CONFIG_FILE);
        DevicesConfig::init_config_file(config_path.clone()).unwrap();
        assert!(config_path.exists());
        assert_eq!(config_path.file_name().unwrap(), "devices.toml");
        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("devices = []"));
    }

    #[test]
    fn test_add_peer() {
        let (config, _temp_dir) = create_temp_devices_config();
        let peer_id = PeerId::random();
        let hostname = Some("test-host".to_string());
        let device_info = DeviceInfo {
            peer_id,
            name: Some("test-device".to_string()),
            hostname: hostname.clone(),
            multiaddrs: vec![],
            os: Os::this_device(),
            public_ip: None,
            private_ips: vec![],
            version: "1.0.0".to_string(),
            created_at: SystemTime::now(),
            last_connected: SystemTime::now(),
        };

        let updated_config = config.add_or_update_device(device_info).unwrap();

        let device_info = updated_config.get_device_info(&peer_id).unwrap();
        assert_eq!(device_info.peer_id, peer_id);
        assert_eq!(device_info.name.as_deref(), Some("test-device"));
        assert_eq!(device_info.hostname, hostname);
    }

    #[test]
    fn test_update_existing_peer() {
        let (config, _temp_dir) = create_temp_devices_config();
        let peer_id = PeerId::random();
        let initial_device_info = DeviceInfo {
            peer_id,
            name: Some("device1".to_string()),
            hostname: Some("host1".to_string()),
            multiaddrs: vec![],
            os: Os::this_device(),
            public_ip: None,
            private_ips: vec![],
            version: "1.0.0".to_string(),
            created_at: SystemTime::now(),
            last_connected: SystemTime::now(),
        };

        let updated_config = config.add_or_update_device(initial_device_info).unwrap();
        let updated_device_info = DeviceInfo {
            peer_id,
            name: Some("name1".to_string()),
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
            .add_or_update_device(updated_device_info)
            .unwrap();
        let device_info = updated_config.get_device_info(&peer_id).unwrap();
        assert_eq!(device_info.name, Some("name1".to_string()));
        assert_eq!(device_info.hostname, Some("host2".to_string()));
        assert_eq!(device_info.version, "1.0.1");
    }

    #[test]
    fn test_add_peer_rejects_duplicate_name_case_insensitive() {
        let (config, _temp_dir) = create_temp_devices_config();

        let first = DeviceInfo {
            peer_id: PeerId::random(),
            name: Some("MacBook".to_string()),
            hostname: None,
            multiaddrs: vec![],
            os: Os::this_device(),
            public_ip: None,
            private_ips: vec![],
            version: "1.0.0".to_string(),
            created_at: SystemTime::now(),
            last_connected: SystemTime::now(),
        };

        let second = DeviceInfo {
            peer_id: PeerId::random(),
            name: Some("  macbook  ".to_string()),
            hostname: None,
            multiaddrs: vec![],
            os: Os::this_device(),
            public_ip: None,
            private_ips: vec![],
            version: "1.0.0".to_string(),
            created_at: SystemTime::now(),
            last_connected: SystemTime::now(),
        };

        let updated = config.add_or_update_device(first).unwrap();
        assert!(updated.add_or_update_device(second).is_err());
    }

    #[test]
    fn test_add_peer_rejects_missing_name() {
        let (config, _temp_dir) = create_temp_devices_config();
        let device_info = DeviceInfo {
            peer_id: PeerId::random(),
            name: None,
            hostname: Some("host1".to_string()),
            multiaddrs: vec![],
            os: Os::this_device(),
            public_ip: None,
            private_ips: vec![],
            version: "1.0.0".to_string(),
            created_at: SystemTime::now(),
            last_connected: SystemTime::now(),
        };

        assert!(config.add_or_update_device(device_info).is_err());
    }

    #[test]
    fn test_add_peer_trims_name_before_saving() {
        let (config, _temp_dir) = create_temp_devices_config();
        let peer_id = PeerId::random();
        let device_info = DeviceInfo {
            peer_id,
            name: Some("  work-laptop  ".to_string()),
            hostname: None,
            multiaddrs: vec![],
            os: Os::this_device(),
            public_ip: None,
            private_ips: vec![],
            version: "1.0.0".to_string(),
            created_at: SystemTime::now(),
            last_connected: SystemTime::now(),
        };

        let updated = config.add_or_update_device(device_info).unwrap();
        assert_eq!(
            updated
                .get_device_info(&peer_id)
                .and_then(|peer| peer.name.clone()),
            Some("work-laptop".to_string())
        );
    }
}
