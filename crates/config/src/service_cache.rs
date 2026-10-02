use std::{
    path::{Path, PathBuf},
    time::SystemTime,
};

use crate::atomic_file::write_atomically;
use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};

const DEVICE_SERVICE_SNAPSHOTS_CACHE_DIR: &str = "cache/device_service_snapshots";

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct DeviceServiceSnapshotCache {
    #[serde(skip)]
    root_dir: PathBuf,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CachedDeviceServiceSnapshot {
    pub peer_id: String,
    pub snapshot_json: String,
    pub updated_at: SystemTime,
}

impl DeviceServiceSnapshotCache {
    pub fn apply_from_dir(fungi_dir: &Path) -> Result<Self> {
        let root_dir = fungi_dir.join(DEVICE_SERVICE_SNAPSHOTS_CACHE_DIR);
        std::fs::create_dir_all(&root_dir).with_context(|| {
            format!(
                "failed to create device service snapshot cache directory: {}",
                root_dir.display()
            )
        })?;
        Ok(Self { root_dir })
    }

    pub fn get_device_snapshot_json(&self, peer_id: &str) -> Result<Option<String>> {
        let path = self.device_cache_path(peer_id);
        if !path.exists() {
            return Ok(None);
        }

        let raw = std::fs::read_to_string(&path).with_context(|| {
            format!(
                "failed to read device service snapshot cache: {}",
                path.display()
            )
        })?;
        let entry: CachedDeviceServiceSnapshot = serde_json::from_str(&raw).with_context(|| {
            format!(
                "failed to parse device service snapshot cache: {}",
                path.display()
            )
        })?;
        Ok(Some(entry.snapshot_json))
    }

    pub fn set_device_snapshot_json(&self, peer_id: String, snapshot_json: String) -> Result<()> {
        std::fs::create_dir_all(&self.root_dir).with_context(|| {
            format!(
                "failed to create device service snapshot cache directory: {}",
                self.root_dir.display()
            )
        })?;
        let entry = CachedDeviceServiceSnapshot {
            peer_id,
            snapshot_json,
            updated_at: SystemTime::now(),
        };
        let raw = serde_json::to_string_pretty(&entry)?;
        let path = self.device_cache_path(&entry.peer_id);
        write_atomically(&path, raw.as_bytes()).with_context(|| {
            format!(
                "failed to write device service snapshot cache: {}",
                path.display()
            )
        })
    }

    pub fn remove_device_snapshot(&self, peer_id: &str) -> Result<bool> {
        let path = self.device_cache_path(peer_id);
        if !path.exists() {
            return Ok(false);
        }
        std::fs::remove_file(&path).with_context(|| {
            format!(
                "failed to remove device service snapshot cache: {}",
                path.display()
            )
        })?;
        Ok(true)
    }

    fn device_cache_path(&self, peer_id: &str) -> PathBuf {
        self.root_dir.join(format!("{peer_id}.json"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn stores_device_service_snapshot_in_unified_cache_file() {
        let dir = TempDir::new().unwrap();
        let cache = DeviceServiceSnapshotCache::apply_from_dir(dir.path()).unwrap();

        cache
            .set_device_snapshot_json("peer-a".to_string(), "{\"services\":[]}".to_string())
            .unwrap();

        let value = cache.get_device_snapshot_json("peer-a").unwrap();
        assert_eq!(value.as_deref(), Some("{\"services\":[]}"));
        assert!(
            dir.path()
                .join("cache")
                .join("device_service_snapshots")
                .join("peer-a.json")
                .exists()
        );
    }

    #[test]
    fn removes_device_service_snapshot_cache_file() {
        let dir = TempDir::new().unwrap();
        let cache = DeviceServiceSnapshotCache::apply_from_dir(dir.path()).unwrap();

        cache
            .set_device_snapshot_json("peer-a".to_string(), "{\"services\":[]}".to_string())
            .unwrap();

        assert!(cache.remove_device_snapshot("peer-a").unwrap());
        assert!(!cache.remove_device_snapshot("peer-a").unwrap());
        assert_eq!(cache.get_device_snapshot_json("peer-a").unwrap(), None);
    }

    #[test]
    fn atomic_write_replaces_existing_cache_file_without_temp_artifacts() {
        let dir = TempDir::new().unwrap();
        let cache = DeviceServiceSnapshotCache::apply_from_dir(dir.path()).unwrap();
        let cache_dir = dir.path().join("cache").join("device_service_snapshots");

        cache
            .set_device_snapshot_json("peer-a".to_string(), "{\"state\":\"old\"}".to_string())
            .unwrap();
        cache
            .set_device_snapshot_json("peer-a".to_string(), "{\"state\":\"new\"}".to_string())
            .unwrap();

        assert_eq!(
            cache.get_device_snapshot_json("peer-a").unwrap().as_deref(),
            Some("{\"state\":\"new\"}")
        );

        let entries = std::fs::read_dir(cache_dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        assert_eq!(entries, vec![std::ffi::OsString::from("peer-a.json")]);
    }
}
