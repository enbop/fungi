use anyhow::Result;
use libp2p_identity::PeerId;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const DEFAULT_TRUSTED_DEVICES_CONFIG_FILE: &str = "trusted_devices.toml";

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct TrustedDevicesConfig {
    #[serde(default)]
    pub trusted_devices: Vec<PeerId>,

    #[serde(skip)]
    config_file: PathBuf,
}

impl TrustedDevicesConfig {
    pub fn in_memory(trusted_devices: Vec<PeerId>) -> Self {
        Self {
            trusted_devices,
            config_file: PathBuf::new(),
        }
    }

    pub fn init_config_file(config_file: PathBuf) -> Result<()> {
        if config_file.exists() {
            return Ok(());
        }
        let config = TrustedDevicesConfig::default();
        let toml_string = toml::to_string(&config)?;
        std::fs::write(&config_file, toml_string)?;
        Ok(())
    }

    pub fn apply_from_dir(fungi_dir: &Path) -> Result<Self> {
        let config_file = fungi_dir.join(DEFAULT_TRUSTED_DEVICES_CONFIG_FILE);
        if !config_file.exists() {
            Self::init_config_file(config_file.clone())?;
        }

        let s = std::fs::read_to_string(&config_file)?;
        let mut cfg = Self::parse_toml(&s)?;
        cfg.config_file = config_file;
        Ok(cfg)
    }

    pub fn parse_toml(s: &str) -> Result<Self> {
        Ok(toml::from_str(s)?)
    }

    pub fn save_to_file(&self) -> Result<()> {
        if self.config_file.as_os_str().is_empty() {
            return Ok(());
        }

        let toml_string = toml::to_string_pretty(self)?;
        std::fs::write(&self.config_file, toml_string)?;
        Ok(())
    }

    pub fn add_trusted_device(&self, peer_id: &PeerId) -> Result<Self> {
        if self.trusted_devices.contains(peer_id) {
            return Ok(self.clone());
        }

        let mut new_config = self.clone();
        new_config.trusted_devices.push(*peer_id);
        new_config.trusted_devices.sort();
        new_config.save_to_file()?;
        Ok(new_config)
    }

    pub fn remove_trusted_device(&self, peer_id: &PeerId) -> Result<Self> {
        let mut new_config = self.clone();
        new_config
            .trusted_devices
            .retain(|trusted_peer_id| trusted_peer_id != peer_id);
        new_config.save_to_file()?;
        Ok(new_config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn stores_trusted_devices_outside_config_toml() {
        let dir = TempDir::new().unwrap();
        let config = TrustedDevicesConfig::apply_from_dir(dir.path()).unwrap();
        let peer_id = PeerId::random();

        let updated = config.add_trusted_device(&peer_id).unwrap();

        assert!(updated.trusted_devices.contains(&peer_id));
        let content =
            std::fs::read_to_string(dir.path().join(DEFAULT_TRUSTED_DEVICES_CONFIG_FILE)).unwrap();
        assert!(content.contains(&peer_id.to_string()));
    }

    #[test]
    fn in_memory_config_can_be_updated_without_a_file() {
        let peer_id = PeerId::random();
        let config = TrustedDevicesConfig::in_memory(vec![]);

        let updated = config.add_trusted_device(&peer_id).unwrap();

        assert_eq!(updated.trusted_devices, vec![peer_id]);
    }
}
