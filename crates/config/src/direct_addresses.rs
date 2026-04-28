use std::{
    path::{Path, PathBuf},
    time::SystemTime,
};

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};

const DIRECT_ADDRESSES_CACHE_FILE: &str = "cache/direct_addresses.json";

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct DirectAddressCache {
    #[serde(default)]
    pub devices: Vec<CachedDeviceAddresses>,

    #[serde(skip)]
    cache_file: PathBuf,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct CachedDeviceAddresses {
    pub peer_id: String,
    #[serde(default)]
    pub addresses: Vec<DirectAddressEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DirectAddressEntry {
    pub address: String,
    pub source: String,
    pub success_count: u64,
    pub first_success_at: SystemTime,
    pub last_success_at: SystemTime,
}

impl DirectAddressCache {
    pub fn apply_from_dir(fungi_dir: &Path) -> Result<Self> {
        let cache_file = fungi_dir.join(DIRECT_ADDRESSES_CACHE_FILE);
        if !cache_file.exists() {
            Self::init_cache_file(cache_file.clone())?;
        }

        let raw = std::fs::read_to_string(&cache_file).with_context(|| {
            format!(
                "failed to read direct address cache: {}",
                cache_file.display()
            )
        })?;
        let mut cache: Self = serde_json::from_str(&raw).with_context(|| {
            format!(
                "failed to parse direct address cache: {}",
                cache_file.display()
            )
        })?;
        cache.cache_file = cache_file;
        Ok(cache)
    }

    pub fn init_cache_file(cache_file: PathBuf) -> Result<()> {
        if let Some(parent) = cache_file.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create direct address cache directory: {}",
                    parent.display()
                )
            })?;
        }
        if cache_file.exists() {
            return Ok(());
        }
        let raw = serde_json::to_string_pretty(&Self::default())?;
        std::fs::write(&cache_file, raw).with_context(|| {
            format!(
                "failed to write direct address cache: {}",
                cache_file.display()
            )
        })?;
        Ok(())
    }

    pub fn save_to_file(&self) -> Result<()> {
        let raw = serde_json::to_string_pretty(self)?;
        std::fs::write(&self.cache_file, raw).with_context(|| {
            format!(
                "failed to write direct address cache: {}",
                self.cache_file.display()
            )
        })
    }

    pub fn get_device_addresses(&self, peer_id: &str) -> Vec<String> {
        self.devices
            .iter()
            .find(|device| device.peer_id == peer_id)
            .map(|device| {
                device
                    .addresses
                    .iter()
                    .map(|entry| entry.address.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn record_successful_addresses<I>(&self, peer_id: String, addresses: I) -> Result<Self>
    where
        I: IntoIterator<Item = String>,
    {
        let mut addresses = addresses
            .into_iter()
            .map(|address| address.trim().to_string())
            .filter(|address| !address.is_empty())
            .collect::<Vec<_>>();
        addresses.sort();
        addresses.dedup();

        if addresses.is_empty() {
            return Ok(self.clone());
        }

        let now = SystemTime::now();
        let mut updated = self.clone();
        let device = match updated
            .devices
            .iter_mut()
            .find(|device| device.peer_id == peer_id)
        {
            Some(device) => device,
            None => {
                updated.devices.push(CachedDeviceAddresses {
                    peer_id: peer_id.clone(),
                    addresses: Vec::new(),
                });
                updated.devices.last_mut().expect("just pushed device")
            }
        };

        for address in addresses {
            match device
                .addresses
                .iter_mut()
                .find(|entry| entry.address == address)
            {
                Some(entry) => {
                    entry.success_count = entry.success_count.saturating_add(1);
                    entry.last_success_at = now;
                }
                None => {
                    device.addresses.push(DirectAddressEntry {
                        address,
                        source: "connection".to_string(),
                        success_count: 1,
                        first_success_at: now,
                        last_success_at: now,
                    });
                }
            }
        }

        device
            .addresses
            .sort_by(|left, right| left.address.cmp(&right.address));
        updated
            .devices
            .sort_by(|left, right| left.peer_id.cmp(&right.peer_id));
        updated.save_to_file()?;
        Ok(updated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devices::{DeviceInfo, DevicesConfig, Os};
    use libp2p_identity::PeerId;
    use tempfile::TempDir;

    #[test]
    fn stores_successful_direct_addresses_outside_devices_toml() {
        let dir = TempDir::new().unwrap();
        let cache = DirectAddressCache::apply_from_dir(dir.path()).unwrap();

        let updated = cache
            .record_successful_addresses(
                "peer-a".to_string(),
                vec![
                    "/ip4/127.0.0.1/tcp/4001".to_string(),
                    "/ip4/127.0.0.1/tcp/4001".to_string(),
                ],
            )
            .unwrap();

        assert_eq!(
            updated.get_device_addresses("peer-a"),
            vec!["/ip4/127.0.0.1/tcp/4001".to_string()]
        );
        assert!(
            dir.path()
                .join("cache")
                .join("direct_addresses.json")
                .exists()
        );
    }

    #[test]
    fn recording_direct_address_does_not_mutate_devices_toml() {
        let dir = TempDir::new().unwrap();
        let devices = DevicesConfig::apply_from_dir(dir.path()).unwrap();
        let peer_id = PeerId::random();
        let devices = devices
            .add_or_update_device(DeviceInfo {
                peer_id,
                name: Some("nas".to_string()),
                hostname: Some("nas.local".to_string()),
                multiaddrs: vec!["/ip4/192.168.1.10/tcp/4001".to_string()],
                private_ips: vec![],
                os: Os::Unknown,
                version: "1.0.0".to_string(),
                public_ip: None,
                created_at: SystemTime::now(),
                last_connected: SystemTime::now(),
            })
            .unwrap();
        let devices_before = std::fs::read_to_string(devices.config_file_path()).unwrap();

        let cache = DirectAddressCache::apply_from_dir(dir.path()).unwrap();
        let updated = cache
            .record_successful_addresses(
                peer_id.to_string(),
                vec!["/ip4/192.168.1.99/tcp/4001".to_string()],
            )
            .unwrap();

        let devices_after = std::fs::read_to_string(devices.config_file_path()).unwrap();
        assert_eq!(devices_after, devices_before);
        assert_eq!(
            updated.get_device_addresses(&peer_id.to_string()),
            vec!["/ip4/192.168.1.99/tcp/4001".to_string()]
        );
        assert!(
            std::fs::read_to_string(dir.path().join("cache").join("direct_addresses.json"))
                .unwrap()
                .contains("192.168.1.99")
        );
    }
}
