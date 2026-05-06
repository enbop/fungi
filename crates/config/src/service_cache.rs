use std::{
    path::{Path, PathBuf},
    time::SystemTime,
};

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};

const DEVICE_PUBLISHED_SERVICES_CACHE_DIR: &str = "cache/remote_services";
const DEVICE_MANAGED_SERVICES_CACHE_DIR: &str = "cache/device_managed_services";

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ServiceCache {
    #[serde(skip)]
    root_dir: PathBuf,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CachedDeviceServices {
    pub peer_id: String,
    pub services_json: String,
    pub updated_at: SystemTime,
}

impl ServiceCache {
    pub fn apply_from_dir(fungi_dir: &Path) -> Result<Self> {
        Self::apply_published_services_from_dir(fungi_dir)
    }

    pub fn apply_published_services_from_dir(fungi_dir: &Path) -> Result<Self> {
        Self::apply_namespace_from_dir(fungi_dir, DEVICE_PUBLISHED_SERVICES_CACHE_DIR)
    }

    pub fn apply_managed_services_from_dir(fungi_dir: &Path) -> Result<Self> {
        Self::apply_namespace_from_dir(fungi_dir, DEVICE_MANAGED_SERVICES_CACHE_DIR)
    }

    fn apply_namespace_from_dir(fungi_dir: &Path, namespace: &str) -> Result<Self> {
        let root_dir = fungi_dir.join(namespace);
        std::fs::create_dir_all(&root_dir).with_context(|| {
            format!(
                "failed to create device services cache directory: {}",
                root_dir.display()
            )
        })?;
        Ok(Self { root_dir })
    }

    pub fn get_device_services_json(&self, peer_id: &str) -> Result<Option<String>> {
        let path = self.device_cache_path(peer_id);
        if !path.exists() {
            return Ok(None);
        }

        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read remote services cache: {}", path.display()))?;
        let entry: CachedDeviceServices = serde_json::from_str(&raw).with_context(|| {
            format!("failed to parse remote services cache: {}", path.display())
        })?;
        Ok(Some(entry.services_json))
    }

    pub fn set_device_services_json(&self, peer_id: String, services_json: String) -> Result<()> {
        std::fs::create_dir_all(&self.root_dir).with_context(|| {
            format!(
                "failed to create device services cache directory: {}",
                self.root_dir.display()
            )
        })?;
        let entry = CachedDeviceServices {
            peer_id,
            services_json,
            updated_at: SystemTime::now(),
        };
        let raw = serde_json::to_string_pretty(&entry)?;
        let path = self.device_cache_path(&entry.peer_id);
        std::fs::write(&path, raw)
            .with_context(|| format!("failed to write remote services cache: {}", path.display()))
    }

    pub fn remove_device_services(&self, peer_id: &str) -> Result<bool> {
        let path = self.device_cache_path(peer_id);
        if !path.exists() {
            return Ok(false);
        }
        std::fs::remove_file(&path).with_context(|| {
            format!("failed to remove remote services cache: {}", path.display())
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
    fn stores_peer_services_in_remote_services_cache_file() {
        let dir = TempDir::new().unwrap();
        let cache = ServiceCache::apply_from_dir(dir.path()).unwrap();

        cache
            .set_device_services_json("peer-a".to_string(), "[{\"id\":\"svc\"}]".to_string())
            .unwrap();

        let value = cache.get_device_services_json("peer-a").unwrap();
        assert_eq!(value.as_deref(), Some("[{\"id\":\"svc\"}]"));
        assert!(
            dir.path()
                .join("cache")
                .join("remote_services")
                .join("peer-a.json")
                .exists()
        );
    }

    #[test]
    fn separates_published_and_managed_device_service_caches() {
        let dir = TempDir::new().unwrap();
        let published = ServiceCache::apply_published_services_from_dir(dir.path()).unwrap();
        let managed = ServiceCache::apply_managed_services_from_dir(dir.path()).unwrap();

        published
            .set_device_services_json("peer-a".to_string(), "[{\"published\":true}]".to_string())
            .unwrap();
        managed
            .set_device_services_json("peer-a".to_string(), "[{\"managed\":true}]".to_string())
            .unwrap();

        assert_eq!(
            published
                .get_device_services_json("peer-a")
                .unwrap()
                .as_deref(),
            Some("[{\"published\":true}]")
        );
        assert_eq!(
            managed
                .get_device_services_json("peer-a")
                .unwrap()
                .as_deref(),
            Some("[{\"managed\":true}]")
        );
        assert!(
            dir.path()
                .join("cache")
                .join("device_managed_services")
                .join("peer-a.json")
                .exists()
        );
    }

    #[test]
    fn removes_device_services_cache_file() {
        let dir = TempDir::new().unwrap();
        let cache = ServiceCache::apply_managed_services_from_dir(dir.path()).unwrap();

        cache
            .set_device_services_json("peer-a".to_string(), "[{\"managed\":true}]".to_string())
            .unwrap();

        assert!(cache.remove_device_services("peer-a").unwrap());
        assert!(!cache.remove_device_services("peer-a").unwrap());
        assert_eq!(cache.get_device_services_json("peer-a").unwrap(), None);
    }
}
