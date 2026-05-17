use anyhow::{Result, bail};
use std::{
    fmt,
    path::{Path, PathBuf},
};

pub const CURRENT_FUNGI_DIR_VERSION: u32 = 2;

pub(crate) const CONFIG_FILE: &str = "config.toml";
pub(crate) const BACKUP_ROOT_DIR: &str = "bk";
pub(crate) const STAGING_DIR_PREFIX: &str = ".fungi-migrate-staging-";
pub(crate) const APPLY_ROLLBACK_DIR_NAME: &str = ".apply-rollback";
pub(crate) const LEGACY_ADDRESS_BOOK_FILE: &str = "address_book.toml";
pub(crate) const DEVICES_FILE: &str = "devices.toml";
pub(crate) const TRUSTED_DEVICES_FILE: &str = "trusted_devices.toml";
pub(crate) const LEGACY_SERVICE_STATE_FILE: &str = "services-state.json";
pub(crate) const SERVICES_ROOT_DIR: &str = "services";
pub(crate) const DATA_ROOT_DIR: &str = "data";
pub(crate) const CURRENT_SERVICE_STATE_SCHEMA_VERSION: u32 = 2;
pub(crate) const LEGACY_SERVICE_STATE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetectedVersion {
    MissingConfig,
    LegacyNoVersion,
    Version(u32),
}

impl DetectedVersion {
    pub(crate) fn backup_label(&self) -> String {
        match self {
            Self::MissingConfig => "missing-config".to_string(),
            Self::LegacyNoVersion => "legacy-no-version".to_string(),
            Self::Version(version) => format!("v{version}"),
        }
    }
}

impl fmt::Display for DetectedVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingConfig => write!(f, "missing config"),
            Self::LegacyNoVersion => write!(f, "legacy config without version"),
            Self::Version(version) => write!(f, "v{version}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationReport {
    pub source_version: DetectedVersion,
    pub target_version: u32,
    pub backup_dir: Option<PathBuf>,
    pub staging_dir: Option<PathBuf>,
    pub migrated_paths: Vec<PathBuf>,
    pub changed: bool,
}

impl MigrationReport {
    pub(crate) fn no_changes(source_version: DetectedVersion) -> Self {
        Self {
            source_version,
            target_version: CURRENT_FUNGI_DIR_VERSION,
            backup_dir: None,
            staging_dir: None,
            migrated_paths: Vec::new(),
            changed: false,
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct MigrationPlan {
    pub(crate) update_config: bool,
    pub(crate) migrate_incoming_allowed_peers: bool,
    pub(crate) migrate_address_book: bool,
    pub(crate) migrate_legacy_managed_services: bool,
    pub(crate) touched_paths: Vec<PathBuf>,
}

impl MigrationPlan {
    pub(crate) fn is_empty(&self) -> bool {
        self.touched_paths.is_empty()
    }
}

pub(crate) fn normalize_non_empty(value: &str, field_name: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("{field_name} must not be empty");
    }
    Ok(trimmed.to_string())
}

pub(crate) fn normalize_optional(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

pub(crate) fn normalize_fs_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::RootDir => normalized.push(component.as_os_str()),
            std::path::Component::Normal(part) => normalized.push(part),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
        }
    }
    normalized
}
