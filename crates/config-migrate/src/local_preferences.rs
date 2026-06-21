use std::{fs, path::Path};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::model::{LEGACY_LOCAL_ACCESS_FILE, LOCAL_PREFERENCES_FILE};

#[derive(Debug, Deserialize)]
struct LegacyLocalAccess {
    #[serde(default)]
    records: Vec<LegacyLocalAccessRecord>,
}

#[derive(Debug, Deserialize)]
struct LegacyLocalAccessRecord {
    remote_peer_id: String,
    remote_service_name: String,
    remote_service_port_name: String,
    local_host: String,
    local_port: u16,
    #[serde(default = "default_local_port_source")]
    local_port_source: String,
    #[serde(default)]
    #[serde(rename = "last_remote_protocol")]
    _last_remote_protocol: Option<String>,
    #[serde(default)]
    #[serde(rename = "last_remote_port")]
    _last_remote_port: Option<u16>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct LocalPreference {
    remote_peer_id: String,
    remote_service_name: String,
    remote_service_port_name: String,
    local_host: String,
    local_port: u16,
    #[serde(default = "default_local_port_source")]
    local_port_source: String,
}

fn default_local_port_source() -> String {
    "auto".to_string()
}

pub(crate) fn migrate_legacy_local_access(staging_root: &Path) -> Result<()> {
    let legacy_path = staging_root.join(LEGACY_LOCAL_ACCESS_FILE);
    let legacy: LegacyLocalAccess =
        serde_json::from_str(&fs::read_to_string(&legacy_path).with_context(|| {
            format!(
                "Failed to read legacy local access file: {}",
                legacy_path.display()
            )
        })?)
        .context("Failed to parse legacy local access records")?;

    let preferences_path = staging_root.join(LOCAL_PREFERENCES_FILE);
    let mut preferences = if preferences_path.exists() {
        serde_json::from_str::<Vec<LocalPreference>>(&fs::read_to_string(&preferences_path)?)
            .context("Failed to parse existing local preferences during migration")?
    } else {
        Vec::new()
    };

    for record in legacy.records {
        let preference = LocalPreference {
            remote_peer_id: record.remote_peer_id,
            remote_service_name: record.remote_service_name,
            remote_service_port_name: record.remote_service_port_name,
            local_host: record.local_host,
            local_port: record.local_port,
            local_port_source: record.local_port_source,
        };
        let exists = preferences.iter().any(|existing| {
            existing.remote_peer_id == preference.remote_peer_id
                && existing.remote_service_name == preference.remote_service_name
                && existing.remote_service_port_name == preference.remote_service_port_name
        });
        if !exists {
            preferences.push(preference);
        }
    }
    preferences.sort_by(|left, right| {
        left.remote_peer_id
            .cmp(&right.remote_peer_id)
            .then(left.remote_service_name.cmp(&right.remote_service_name))
            .then(
                left.remote_service_port_name
                    .cmp(&right.remote_service_port_name),
            )
    });

    if let Some(parent) = preferences_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&preferences_path, serde_json::to_vec_pretty(&preferences)?)?;
    fs::remove_file(&legacy_path)?;
    Ok(())
}
