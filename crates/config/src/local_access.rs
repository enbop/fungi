use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use serde::{Deserialize, Serialize};

const LOCAL_ACCESS_FILE: &str = "access/local_access.json";

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct LocalAccessConfig {
    #[serde(default)]
    pub records: Vec<LocalAccessRecord>,

    #[serde(skip)]
    config_file: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct LocalAccessRecord {
    pub remote_peer_id: String,
    pub remote_service_name: String,
    pub remote_service_port_name: String,
    pub local_host: String,
    pub local_port: u16,
    #[serde(default)]
    pub local_port_source: LocalPortSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_remote_protocol: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_remote_port: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalPortSource {
    #[default]
    Auto,
    User,
}

impl LocalAccessConfig {
    pub fn apply_from_dir(fungi_dir: &Path) -> Result<Self> {
        let config_file = fungi_dir.join(LOCAL_ACCESS_FILE);
        if !config_file.exists() {
            Self::init_config_file(config_file.clone())?;
        }

        let raw = std::fs::read_to_string(&config_file).with_context(|| {
            format!(
                "failed to read local access config: {}",
                config_file.display()
            )
        })?;
        let mut config: Self = serde_json::from_str(&raw).with_context(|| {
            format!(
                "failed to parse local access config: {}",
                config_file.display()
            )
        })?;
        config.config_file = config_file;
        Ok(config)
    }

    pub fn init_config_file(config_file: PathBuf) -> Result<()> {
        if config_file.exists() {
            return Ok(());
        }
        if let Some(parent) = config_file.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create local access directory: {}",
                    parent.display()
                )
            })?;
        }
        let raw = serde_json::to_string_pretty(&Self::default())?;
        std::fs::write(&config_file, raw).with_context(|| {
            format!(
                "failed to write local access config: {}",
                config_file.display()
            )
        })?;
        Ok(())
    }

    pub fn save_to_file(&self) -> Result<()> {
        let raw = serde_json::to_string_pretty(self)?;
        std::fs::write(&self.config_file, raw).with_context(|| {
            format!(
                "failed to write local access config: {}",
                self.config_file.display()
            )
        })?;
        Ok(())
    }

    pub fn find_record(
        &self,
        remote_peer_id: &str,
        remote_service_name: &str,
        remote_service_port_name: &str,
    ) -> Option<&LocalAccessRecord> {
        self.records.iter().find(|record| {
            record.identity_matches(
                remote_peer_id,
                remote_service_name,
                remote_service_port_name,
            )
        })
    }

    pub fn upsert_record(&self, record: LocalAccessRecord) -> Result<Self> {
        record.validate()?;
        if self.local_port_used_by_other_record(&record) {
            bail!(
                "local port is already reserved by another service access: {}",
                record.local_port
            );
        }

        let mut updated = self.clone();
        if let Some(existing) = updated
            .records
            .iter_mut()
            .find(|existing| existing.same_identity(&record))
        {
            *existing = record;
        } else {
            updated.records.push(record);
        }
        updated.sort_records();
        updated.save_to_file()?;
        Ok(updated)
    }

    pub fn remove_service_records(
        &self,
        remote_peer_id: &str,
        remote_service_name: &str,
    ) -> Result<Self> {
        let mut updated = self.clone();
        updated.records.retain(|record| {
            !(record.remote_peer_id == remote_peer_id
                && record.remote_service_name == remote_service_name)
        });
        updated.save_to_file()?;
        Ok(updated)
    }

    pub fn remove_device_records(&self, remote_peer_id: &str) -> Result<Self> {
        let mut updated = self.clone();
        updated
            .records
            .retain(|record| record.remote_peer_id != remote_peer_id);
        updated.save_to_file()?;
        Ok(updated)
    }

    fn local_port_used_by_other_record(&self, record: &LocalAccessRecord) -> bool {
        self.records.iter().any(|existing| {
            existing.local_host == record.local_host
                && existing.local_port == record.local_port
                && !existing.same_identity(record)
        })
    }

    fn sort_records(&mut self) {
        self.records.sort_by(|left, right| {
            left.remote_peer_id
                .cmp(&right.remote_peer_id)
                .then(left.remote_service_name.cmp(&right.remote_service_name))
                .then(
                    left.remote_service_port_name
                        .cmp(&right.remote_service_port_name),
                )
                .then(left.local_host.cmp(&right.local_host))
                .then(left.local_port.cmp(&right.local_port))
        });
    }
}

impl LocalAccessRecord {
    pub fn identity_matches(
        &self,
        remote_peer_id: &str,
        remote_service_name: &str,
        remote_service_port_name: &str,
    ) -> bool {
        self.remote_peer_id == remote_peer_id
            && self.remote_service_name == remote_service_name
            && self.remote_service_port_name == remote_service_port_name
    }

    fn same_identity(&self, other: &Self) -> bool {
        self.identity_matches(
            &other.remote_peer_id,
            &other.remote_service_name,
            &other.remote_service_port_name,
        )
    }

    fn validate(&self) -> Result<()> {
        validate_non_empty("remote_peer_id", &self.remote_peer_id)?;
        validate_non_empty("remote_service_name", &self.remote_service_name)?;
        validate_non_empty("remote_service_port_name", &self.remote_service_port_name)?;
        validate_non_empty("local_host", &self.local_host)?;
        if self.local_port == 0 {
            bail!("local_port must be greater than 0");
        }
        Ok(())
    }
}

fn validate_non_empty(field: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        bail!("{field} must not be empty");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn stores_local_access_records_outside_config_toml() {
        let dir = TempDir::new().unwrap();
        let config = LocalAccessConfig::apply_from_dir(dir.path()).unwrap();

        let updated = config
            .upsert_record(record("peer", "svc", "main", 2222))
            .unwrap();

        assert_eq!(updated.records.len(), 1);
        assert!(dir.path().join("access").join("local_access.json").exists());
        let raw =
            std::fs::read_to_string(dir.path().join("access").join("local_access.json")).unwrap();
        assert!(raw.contains("\"records\""));
        assert!(!raw.contains("\"rules\""));
    }

    #[test]
    fn upsert_updates_same_service_entry_port() {
        let dir = TempDir::new().unwrap();
        let config = LocalAccessConfig::apply_from_dir(dir.path()).unwrap();

        let updated = config
            .upsert_record(record("peer", "svc", "main", 2222))
            .unwrap()
            .upsert_record(record("peer", "svc", "main", 3333))
            .unwrap();

        assert_eq!(updated.records.len(), 1);
        assert_eq!(updated.records[0].local_port, 3333);
    }

    #[test]
    fn keeps_same_endpoint_names_for_different_services() {
        let dir = TempDir::new().unwrap();
        let config = LocalAccessConfig::apply_from_dir(dir.path()).unwrap();

        let updated = config
            .upsert_record(record("peer", "alpha", "main", 2222))
            .unwrap()
            .upsert_record(record("peer", "beta", "main", 3333))
            .unwrap();

        assert_eq!(updated.records.len(), 2);
        assert!(updated.find_record("peer", "alpha", "main").is_some());
        assert!(updated.find_record("peer", "beta", "main").is_some());
    }

    #[test]
    fn rejects_duplicate_local_ports_for_different_entries() {
        let dir = TempDir::new().unwrap();
        let config = LocalAccessConfig::apply_from_dir(dir.path()).unwrap();

        let err = config
            .upsert_record(record("peer", "alpha", "main", 2222))
            .unwrap()
            .upsert_record(record("peer", "beta", "main", 2222))
            .unwrap_err();

        assert!(
            err.to_string()
                .contains("local port is already reserved by another service access")
        );
    }

    #[test]
    fn removes_service_records_without_touching_other_services() {
        let dir = TempDir::new().unwrap();
        let config = LocalAccessConfig::apply_from_dir(dir.path()).unwrap();

        let updated = config
            .upsert_record(record("peer", "alpha", "main", 2222))
            .unwrap()
            .upsert_record(record("peer", "beta", "main", 3333))
            .unwrap()
            .remove_service_records("peer", "alpha")
            .unwrap();

        assert!(updated.find_record("peer", "alpha", "main").is_none());
        assert!(updated.find_record("peer", "beta", "main").is_some());
    }

    #[test]
    fn removes_device_records() {
        let dir = TempDir::new().unwrap();
        let config = LocalAccessConfig::apply_from_dir(dir.path()).unwrap();

        let updated = config
            .upsert_record(record("peer-a", "alpha", "main", 2222))
            .unwrap()
            .upsert_record(record("peer-b", "beta", "main", 3333))
            .unwrap()
            .remove_device_records("peer-a")
            .unwrap();

        assert!(updated.find_record("peer-a", "alpha", "main").is_none());
        assert!(updated.find_record("peer-b", "beta", "main").is_some());
    }

    fn record(
        remote_peer_id: &str,
        remote_service_name: &str,
        remote_service_port_name: &str,
        local_port: u16,
    ) -> LocalAccessRecord {
        LocalAccessRecord {
            remote_peer_id: remote_peer_id.to_string(),
            remote_service_name: remote_service_name.to_string(),
            remote_service_port_name: remote_service_port_name.to_string(),
            local_host: "127.0.0.1".to_string(),
            local_port,
            local_port_source: LocalPortSource::Auto,
            last_remote_protocol: Some(format!(
                "/fungi/service/{remote_service_name}/{remote_service_port_name}/0.2.0"
            )),
            last_remote_port: Some(22),
        }
    }
}
