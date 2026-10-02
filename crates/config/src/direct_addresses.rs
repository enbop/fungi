use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use crate::atomic_file::write_atomically;
use anyhow::{Context as _, Result};
use fungi_util::address_policy::{
    MAX_CACHED_ADDRESSES_PER_PEER, address_bucket, address_within_retention, retain_diverse,
};
use serde::{Deserialize, Serialize};

const ACTIVE_ADDRESS_REFRESH_INTERVAL: Duration = Duration::from_secs(60 * 60);

const DIRECT_ADDRESSES_CACHE_FILE: &str = "cache/direct_addresses.json";

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct DirectAddressCache {
    #[serde(default)]
    pub devices: Vec<CachedDeviceAddresses>,

    #[serde(skip)]
    cache_file: PathBuf,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct CachedDeviceAddresses {
    pub peer_id: String,
    #[serde(default)]
    pub addresses: Vec<DirectAddressEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
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
        if cache.prune(SystemTime::now()) {
            cache.save_to_file()?;
        }
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
        write_atomically(&self.cache_file, raw.as_bytes()).with_context(|| {
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

    /// Compatibility wrapper for callers that want an immediately persisted update.
    pub fn record_successful_addresses<I>(&self, peer_id: String, addresses: I) -> Result<Self>
    where
        I: IntoIterator<Item = String>,
    {
        let now = SystemTime::now();
        let mut updated = self.clone();
        let recorded = updated.record_successes(peer_id, addresses, now);
        let pruned = updated.prune(now);
        if recorded || pruned {
            updated.save_to_file()?;
        }
        Ok(updated)
    }

    /// Merge observations in memory. The daemon manager batches persistence separately.
    pub fn record_successes<I>(&mut self, peer_id: String, addresses: I, now: SystemTime) -> bool
    where
        I: IntoIterator<Item = String>,
    {
        let mut addresses: Vec<_> = addresses
            .into_iter()
            .map(|address| address.trim().to_owned())
            .filter(|address| !address.is_empty())
            .collect();
        addresses.sort();
        addresses.dedup();
        if addresses.is_empty() {
            return false;
        }
        let index = self
            .devices
            .iter()
            .position(|device| device.peer_id == peer_id)
            .unwrap_or_else(|| {
                self.devices.push(CachedDeviceAddresses {
                    peer_id,
                    addresses: Vec::new(),
                });
                self.devices.len() - 1
            });
        let device = &mut self.devices[index];
        for address in addresses {
            if let Some(entry) = device
                .addresses
                .iter_mut()
                .find(|entry| entry.address == address)
            {
                entry.success_count = entry.success_count.saturating_add(1);
                entry.last_success_at = now;
            } else {
                device.addresses.push(DirectAddressEntry {
                    address,
                    source: "connection".to_owned(),
                    success_count: 1,
                    first_success_at: now,
                    last_success_at: now,
                });
            }
        }
        true
    }

    /// Long-lived direct connections still prove reachability. Refresh their age at
    /// most hourly without inflating the reconnect counter or writing every tick.
    pub fn refresh_active_addresses(
        &mut self,
        peer_id: &str,
        addresses: &[String],
        now: SystemTime,
    ) -> bool {
        let Some(device) = self
            .devices
            .iter_mut()
            .find(|device| device.peer_id == peer_id)
        else {
            return false;
        };
        let mut changed = false;
        for entry in &mut device.addresses {
            if addresses.contains(&entry.address)
                && now
                    .duration_since(entry.last_success_at)
                    .unwrap_or_default()
                    >= ACTIVE_ADDRESS_REFRESH_INTERVAL
            {
                entry.last_success_at = now;
                changed = true;
            }
        }
        changed
    }

    /// Apply the same policy to legacy files, new observations and idle maintenance.
    pub fn prune(&mut self, now: SystemTime) -> bool {
        let before = self.devices.clone();
        for device in &mut self.devices {
            device
                .addresses
                .retain(|entry| address_within_retention(entry.last_success_at, now));
            // Collapse legacy duplicates, retaining the latest successful observation.
            device.addresses.sort_by(|a, b| {
                a.address
                    .cmp(&b.address)
                    .then(b.last_success_at.cmp(&a.last_success_at))
            });
            device.addresses.dedup_by(|a, b| a.address == b.address);
            device.addresses.sort_by(|a, b| {
                b.last_success_at
                    .cmp(&a.last_success_at)
                    .then(a.address.cmp(&b.address))
            });
            retain_diverse(
                &mut device.addresses,
                MAX_CACHED_ADDRESSES_PER_PEER,
                |entry| entry.address.parse().ok().as_ref().map(address_bucket),
            );
            // Preserve the existing stable on-disk order and JSON schema.
            device.addresses.sort_by(|a, b| a.address.cmp(&b.address));
        }
        self.devices.retain(|device| !device.addresses.is_empty());
        self.devices.sort_by(|a, b| a.peer_id.cmp(&b.peer_id));
        self.devices != before
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
    #[test]
    fn loading_legacy_cache_prunes_disk_and_preserves_recent_transport_choices() {
        use fungi_util::address_policy::DIRECT_ADDRESS_RETENTION;
        let dir = TempDir::new().unwrap();
        let mut cache = DirectAddressCache::apply_from_dir(dir.path()).unwrap();
        let now = SystemTime::now();
        // Reproduce accumulated IPv6/QUIC history, with newer observations on
        // lexicographically later addresses. TCP and IPv4 must retain a slot.
        for index in 0..140 {
            cache.record_successes(
                "peer-a".into(),
                vec![format!("/ip6/2001:db8::{:x}/udp/4001/quic-v1", index + 1)],
                now - Duration::from_secs(200 - index),
            );
        }
        let ipv4 = "/ip4/192.168.1.145/udp/5001/quic-v1".to_owned();
        let tcp = "/ip6/2001:db8::ffff/tcp/5002".to_owned();
        cache.record_successes(
            "peer-a".into(),
            vec![ipv4.clone(), tcp.clone()],
            now - Duration::from_secs(250),
        );
        cache.record_successes(
            "expired-peer".into(),
            vec!["/ip4/192.168.1.9/tcp/1".into()],
            now - DIRECT_ADDRESS_RETENTION - Duration::from_secs(1),
        );
        cache.save_to_file().unwrap();

        let loaded = DirectAddressCache::apply_from_dir(dir.path()).unwrap();
        let addresses = loaded.get_device_addresses("peer-a");
        assert_eq!(addresses.len(), MAX_CACHED_ADDRESSES_PER_PEER);
        assert!(addresses.contains(&ipv4));
        assert!(addresses.contains(&tcp));
        assert!(addresses.contains(&"/ip6/2001:db8::8c/udp/4001/quic-v1".into()));
        assert!(!addresses.contains(&"/ip6/2001:db8::1/udp/4001/quic-v1".into()));
        assert!(loaded.get_device_addresses("expired-peer").is_empty());
        let persisted: DirectAddressCache =
            serde_json::from_str(&std::fs::read_to_string(&loaded.cache_file).unwrap()).unwrap();
        assert_eq!(persisted.devices, loaded.devices);
        assert!(
            loaded.devices[0]
                .addresses
                .iter()
                .all(|entry| entry.source == "connection" && entry.success_count == 1)
        );
        let mut same = loaded.clone();
        assert!(!same.prune(now));
    }

    #[test]
    fn cleanup_boundary_and_active_refresh_preserve_success_count() {
        use fungi_util::address_policy::DIRECT_ADDRESS_RETENTION;
        let now = SystemTime::now();
        let address = "/ip4/192.168.1.145/tcp/5001".to_owned();
        let mut cache = DirectAddressCache::default();
        cache.record_successes("peer".into(), vec![address.clone()], now);
        assert!(!cache.refresh_active_addresses(
            "peer",
            &[address.clone()],
            now + Duration::from_secs(30)
        ));
        assert!(!cache.prune(now + DIRECT_ADDRESS_RETENTION));
        let later = now + DIRECT_ADDRESS_RETENTION + Duration::from_secs(1);
        assert!(cache.refresh_active_addresses("peer", &[address.clone()], later));
        assert!(!cache.prune(later));
        assert_eq!(cache.devices[0].addresses[0].success_count, 1);
        assert_eq!(cache.devices[0].addresses[0].last_success_at, later);
        assert!(!cache.prune(now)); // tolerate a wall-clock rollback
        assert!(cache.prune(later + DIRECT_ADDRESS_RETENTION + Duration::from_secs(1)));
        assert!(cache.devices.is_empty());
    }
}
