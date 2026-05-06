use anyhow::{Context, Result};
use std::{collections::BTreeSet, fs, path::Path};

use crate::model::{
    CONFIG_FILE, CURRENT_FUNGI_DIR_VERSION, DEVICES_FILE, LEGACY_ADDRESS_BOOK_FILE,
    SERVICES_ROOT_DIR, normalize_fs_path,
};

pub(crate) fn migrate_config_toml_to_current(staging_root: &Path, fungi_dir: &Path) -> Result<()> {
    let config_path = staging_root.join(CONFIG_FILE);
    let content = fs::read_to_string(&config_path).with_context(|| {
        format!(
            "Failed to read config.toml from staging directory: {}",
            config_path.display()
        )
    })?;
    let mut value: toml::Value =
        toml::from_str(&content).context("Failed to parse config.toml in staging")?;
    let Some(table) = value.as_table_mut() else {
        anyhow::bail!("config.toml root must be a TOML table");
    };

    table.insert(
        "version".to_string(),
        toml::Value::Integer(CURRENT_FUNGI_DIR_VERSION.into()),
    );
    table.remove("incoming_allowed_peers");

    if let Some(runtime_table) = table.get_mut("runtime").and_then(toml::Value::as_table_mut) {
        runtime_table.remove("allowed_ports");
        runtime_table.remove("allowed_port_ranges");
        remove_legacy_services_allowlist(runtime_table, fungi_dir);
    }

    let updated = toml::to_string_pretty(&value)
        .context("Failed to encode migrated config.toml from staging")?;
    fs::write(&config_path, updated).with_context(|| {
        format!(
            "Failed to write migrated config.toml into staging directory: {}",
            config_path.display()
        )
    })?;

    Ok(())
}

pub(crate) fn migrate_legacy_address_book(staging_root: &Path) -> Result<()> {
    let address_book_path = staging_root.join(LEGACY_ADDRESS_BOOK_FILE);
    if !address_book_path.exists() {
        return Ok(());
    }

    let content = fs::read_to_string(&address_book_path).with_context(|| {
        format!(
            "Failed to read legacy address_book.toml from staging directory: {}",
            address_book_path.display()
        )
    })?;
    let address_book_value: toml::Value =
        toml::from_str(&content).context("Failed to parse legacy address_book.toml in staging")?;
    let peers = address_book_value
        .as_table()
        .and_then(|table| table.get("peers"))
        .and_then(toml::Value::as_array)
        .cloned()
        .unwrap_or_default();

    let devices_path = staging_root.join(DEVICES_FILE);
    let mut devices_value = if devices_path.exists() {
        let existing = fs::read_to_string(&devices_path).with_context(|| {
            format!(
                "Failed to read existing devices.toml from staging directory: {}",
                devices_path.display()
            )
        })?;
        toml::from_str(&existing).context("Failed to parse existing devices.toml in staging")?
    } else {
        toml::Value::Table(toml::map::Map::new())
    };

    let devices_array = ensure_toml_array_field(&mut devices_value, "devices")?;
    let mut existing_peer_ids = devices_array
        .iter()
        .filter_map(|value| value.as_table())
        .filter_map(|table| table.get("peer_id"))
        .filter_map(toml::Value::as_str)
        .map(ToOwned::to_owned)
        .collect::<BTreeSet<_>>();

    for peer in peers {
        let peer_table = peer
            .as_table()
            .ok_or_else(|| anyhow::anyhow!("legacy address_book peers must be TOML tables"))?;
        let peer_id = required_toml_string(peer_table, "peer_id")?;
        if existing_peer_ids.contains(peer_id) {
            continue;
        }

        devices_array.push(toml::Value::Table(legacy_peer_to_device_table(peer_table)?));
        existing_peer_ids.insert(peer_id.to_string());
    }

    let encoded = toml::to_string_pretty(&devices_value)
        .context("Failed to encode migrated devices.toml from staging")?;
    fs::write(&devices_path, encoded).with_context(|| {
        format!(
            "Failed to write migrated devices.toml into staging directory: {}",
            devices_path.display()
        )
    })?;

    fs::remove_file(&address_book_path).with_context(|| {
        format!(
            "Failed to remove legacy address_book.toml from staging directory: {}",
            address_book_path.display()
        )
    })?;
    Ok(())
}

fn remove_legacy_services_allowlist(runtime_table: &mut toml::Table, fungi_dir: &Path) {
    let legacy_services_path = normalize_fs_path(&fungi_dir.join(SERVICES_ROOT_DIR));
    let Some(paths) = runtime_table
        .get_mut("allowed_host_paths")
        .and_then(toml::Value::as_array_mut)
    else {
        return;
    };

    paths.retain(|value| {
        !value
            .as_str()
            .is_some_and(|raw| normalize_fs_path(Path::new(raw)) == legacy_services_path)
    });
    if paths.is_empty() {
        runtime_table.remove("allowed_host_paths");
    }
}

fn ensure_toml_array_field<'a>(
    value: &'a mut toml::Value,
    key: &str,
) -> Result<&'a mut Vec<toml::Value>> {
    let table = value
        .as_table_mut()
        .ok_or_else(|| anyhow::anyhow!("expected TOML table while accessing '{key}'"))?;
    if !table.contains_key(key) {
        table.insert(key.to_string(), toml::Value::Array(Vec::new()));
    }

    table
        .get_mut(key)
        .and_then(toml::Value::as_array_mut)
        .ok_or_else(|| anyhow::anyhow!("expected TOML array for '{key}'"))
}

fn legacy_peer_to_device_table(peer_table: &toml::Table) -> Result<toml::Table> {
    let mut device_table = toml::map::Map::new();
    device_table.insert(
        "peer_id".to_string(),
        peer_table
            .get("peer_id")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("legacy address_book peer is missing peer_id"))?,
    );
    if let Some(alias) = peer_table.get("alias").cloned() {
        device_table.insert("name".to_string(), alias);
    }
    if let Some(hostname) = peer_table.get("hostname").cloned() {
        device_table.insert("hostname".to_string(), hostname);
    }
    device_table.insert("multiaddrs".to_string(), toml::Value::Array(Vec::new()));
    device_table.insert(
        "private_ips".to_string(),
        peer_table
            .get("private_ips")
            .cloned()
            .unwrap_or_else(|| toml::Value::Array(Vec::new())),
    );
    device_table.insert(
        "os".to_string(),
        peer_table
            .get("os")
            .cloned()
            .unwrap_or_else(|| toml::Value::String("Unknown".to_string())),
    );
    device_table.insert(
        "version".to_string(),
        peer_table
            .get("version")
            .cloned()
            .unwrap_or_else(|| toml::Value::String(String::new())),
    );
    if let Some(public_ip) = peer_table.get("public_ip").cloned() {
        device_table.insert("public_ip".to_string(), public_ip);
    }
    device_table.insert(
        "created_at".to_string(),
        peer_table
            .get("created_at")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("legacy address_book peer is missing created_at"))?,
    );
    device_table.insert(
        "last_connected".to_string(),
        peer_table
            .get("last_connected")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("legacy address_book peer is missing last_connected"))?,
    );
    Ok(device_table)
}

fn required_toml_string<'a>(table: &'a toml::Table, key: &str) -> Result<&'a str> {
    table
        .get(key)
        .and_then(toml::Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("expected string field '{key}' in legacy TOML table"))
}
