use anyhow::{Context, Result, bail};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use crate::model::{
    CONFIG_FILE, DATA_ROOT_DIR, DEVICES_FILE, DetectedVersion, LEGACY_ADDRESS_BOOK_FILE,
    LEGACY_SERVICE_STATE_FILE, MigrationPlan, SERVICES_ROOT_DIR, TRUSTED_DEVICES_FILE,
    normalize_fs_path,
};

pub fn detect_source_version(fungi_dir: &Path) -> Result<DetectedVersion> {
    let config_path = fungi_dir.join(CONFIG_FILE);
    if !config_path.exists() {
        return Ok(DetectedVersion::MissingConfig);
    }

    let content = fs::read_to_string(&config_path).with_context(|| {
        format!(
            "Failed to read Fungi config file during migration detection: {}",
            config_path.display()
        )
    })?;
    detect_source_version_from_toml_str(&content)
}

pub(crate) fn detect_source_version_from_toml_str(content: &str) -> Result<DetectedVersion> {
    let value: toml::Value = toml::from_str(content)
        .context("Failed to parse config.toml during migration detection")?;
    let Some(table) = value.as_table() else {
        bail!("config.toml root must be a TOML table for migration detection");
    };

    let Some(version) = table.get("version") else {
        return Ok(DetectedVersion::LegacyNoVersion);
    };

    let Some(version) = version.as_integer() else {
        bail!("config.toml version must be an integer for migration detection");
    };
    if version < 0 {
        bail!("config.toml version must not be negative for migration detection");
    }

    Ok(DetectedVersion::Version(version as u32))
}

pub(crate) fn build_migration_plan(
    fungi_dir: &Path,
    source_version: &DetectedVersion,
) -> Result<MigrationPlan> {
    let legacy_address_book_exists = fungi_dir.join(LEGACY_ADDRESS_BOOK_FILE).exists();
    let legacy_service_state_exists = fungi_dir.join(LEGACY_SERVICE_STATE_FILE).exists();

    if matches!(source_version, DetectedVersion::MissingConfig)
        && (legacy_address_book_exists || legacy_service_state_exists)
    {
        bail!(
            "Found legacy Fungi side files without config.toml in {}. Initialize or restore the fungi-dir before migrating.",
            fungi_dir.display()
        );
    }

    if legacy_service_state_exists {
        ensure_no_mixed_managed_service_layout(fungi_dir)?;
    }

    let mut touched_paths = BTreeSet::new();
    let migrate_incoming_allowed_peers =
        config_has_legacy_incoming_allowed_peers(fungi_dir, source_version)?;
    let update_config = config_requires_current_normalization(fungi_dir, source_version)?
        || migrate_incoming_allowed_peers;
    if update_config {
        touched_paths.insert(PathBuf::from(CONFIG_FILE));
    }
    if migrate_incoming_allowed_peers {
        touched_paths.insert(PathBuf::from(TRUSTED_DEVICES_FILE));
    }

    if legacy_address_book_exists {
        touched_paths.insert(PathBuf::from(LEGACY_ADDRESS_BOOK_FILE));
        touched_paths.insert(PathBuf::from(DEVICES_FILE));
    }

    if legacy_service_state_exists {
        touched_paths.insert(PathBuf::from(LEGACY_SERVICE_STATE_FILE));
        touched_paths.insert(PathBuf::from(SERVICES_ROOT_DIR));
        touched_paths.insert(PathBuf::from(DATA_ROOT_DIR));
    }

    Ok(MigrationPlan {
        update_config,
        migrate_incoming_allowed_peers,
        migrate_address_book: legacy_address_book_exists,
        migrate_legacy_managed_services: legacy_service_state_exists,
        touched_paths: touched_paths.into_iter().collect(),
    })
}

fn config_has_legacy_incoming_allowed_peers(
    fungi_dir: &Path,
    source_version: &DetectedVersion,
) -> Result<bool> {
    if matches!(source_version, DetectedVersion::MissingConfig) {
        return Ok(false);
    }

    let table = read_config_table(fungi_dir, "detecting legacy incoming peer allowlist")?;
    Ok(table.contains_key("incoming_allowed_peers")
        || table
            .get("network")
            .and_then(toml::Value::as_table)
            .is_some_and(|network_table| network_table.contains_key("incoming_allowed_peers")))
}

fn config_requires_current_normalization(
    fungi_dir: &Path,
    source_version: &DetectedVersion,
) -> Result<bool> {
    if matches!(source_version, DetectedVersion::MissingConfig) {
        return Ok(false);
    }

    let table = read_config_table(fungi_dir, "building migration plan")?;

    if !table.contains_key("version") {
        return Ok(true);
    }

    let Some(runtime_table) = table.get("runtime").and_then(toml::Value::as_table) else {
        return Ok(false);
    };

    if runtime_table.contains_key("allowed_ports")
        || runtime_table.contains_key("allowed_port_ranges")
    {
        return Ok(true);
    }

    let legacy_services_path = normalize_fs_path(&fungi_dir.join(SERVICES_ROOT_DIR));
    let has_legacy_services_allowlist = runtime_table
        .get("allowed_host_paths")
        .and_then(toml::Value::as_array)
        .is_some_and(|paths| {
            paths.iter().any(|value| {
                value
                    .as_str()
                    .is_some_and(|raw| normalize_fs_path(Path::new(raw)) == legacy_services_path)
            })
        });

    Ok(has_legacy_services_allowlist)
}

fn read_config_table(fungi_dir: &Path, action: &str) -> Result<toml::Table> {
    let config_path = fungi_dir.join(CONFIG_FILE);
    let content = fs::read_to_string(&config_path).with_context(|| {
        format!(
            "Failed to read Fungi config file while {action}: {}",
            config_path.display()
        )
    })?;
    let value: toml::Value = toml::from_str(&content)
        .with_context(|| format!("Failed to parse config.toml while {action}"))?;
    let Some(table) = value.as_table() else {
        bail!("config.toml root must be a TOML table while {action}");
    };
    Ok(table.clone())
}

fn ensure_no_mixed_managed_service_layout(fungi_dir: &Path) -> Result<()> {
    let services_root = fungi_dir.join(SERVICES_ROOT_DIR);
    if !services_root.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(&services_root).with_context(|| {
        format!(
            "Failed to inspect services directory while planning migration: {}",
            services_root.display()
        )
    })? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }

        let path = entry.path();
        if path.join("service.yaml").exists() || path.join("state.json").exists() {
            bail!(
                "Unsupported mixed managed service layouts detected in {}. Legacy services-state.json cannot be merged automatically with current services/<local_service_id>/ entries yet.",
                services_root.display()
            );
        }
    }

    Ok(())
}
