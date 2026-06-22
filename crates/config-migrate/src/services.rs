use anyhow::{Context, Result, bail};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};
use ulid::Ulid;

use crate::model::{
    APPDATA_SERVICES_ROOT_DIR, ARTIFACTS_SERVICES_ROOT_DIR, CURRENT_SERVICE_STATE_SCHEMA_VERSION,
    LEGACY_SERVICE_STATE_FILE, LEGACY_SERVICE_STATE_SCHEMA_VERSION, SERVICES_ROOT_DIR,
    normalize_non_empty, normalize_optional,
};

pub(crate) fn migrate_legacy_services_state(staging_root: &Path, live_root: &Path) -> Result<()> {
    let legacy_state_path = staging_root.join(LEGACY_SERVICE_STATE_FILE);
    if !legacy_state_path.exists() {
        return Ok(());
    }

    let content = fs::read_to_string(&legacy_state_path).with_context(|| {
        format!(
            "Failed to read legacy services-state.json from staging directory: {}",
            legacy_state_path.display()
        )
    })?;
    let legacy_state: LegacyServiceStateFile = serde_json::from_str(&content)
        .context("Failed to parse legacy services-state.json in staging")?;
    if legacy_state.schema_version != LEGACY_SERVICE_STATE_SCHEMA_VERSION {
        bail!(
            "Unsupported legacy service state schema version {} in {}",
            legacy_state.schema_version,
            legacy_state_path.display()
        );
    }

    let legacy_updated_at = normalize_optional(Some(legacy_state.updated_at))
        .unwrap_or_else(|| Utc::now().to_rfc3339());
    let services_root = staging_root.join(SERVICES_ROOT_DIR);
    let appdata_root = staging_root.join(APPDATA_SERVICES_ROOT_DIR);
    let artifacts_root = staging_root.join(ARTIFACTS_SERVICES_ROOT_DIR);
    let mut allocated_local_service_ids = BTreeSet::new();

    for (service_name_key, persisted_service) in legacy_state.services {
        let service_name =
            normalize_non_empty(&persisted_service.manifest.name, "legacy manifest.name")?;
        if service_name_key != service_name {
            bail!(
                "Legacy services-state entry '{}' does not match manifest name '{}'",
                service_name_key,
                service_name
            );
        }

        let local_service_id = generate_unique_local_service_id(&mut allocated_local_service_ids)?;
        let old_service_data_dir = services_root.join(&service_name);
        let old_live_service_data_dir = live_root.join(SERVICES_ROOT_DIR).join(&service_name);
        let new_service_appdata_dir = appdata_root.join(&local_service_id);
        let new_service_artifacts_dir = artifacts_root.join(&local_service_id);
        move_or_create_service_data_dir(&old_service_data_dir, &new_service_appdata_dir)?;

        let manifest_document = migrate_legacy_service_manifest(
            persisted_service.manifest,
            &old_live_service_data_dir,
            &new_service_appdata_dir,
            &new_service_artifacts_dir,
        )?;
        let service_dir = services_root.join(&local_service_id);
        fs::create_dir_all(&service_dir).with_context(|| {
            format!(
                "Failed to create migrated managed service directory in staging: {}",
                service_dir.display()
            )
        })?;

        let manifest_yaml = serde_yaml::to_string(&manifest_document)
            .context("Failed to encode migrated managed service manifest YAML")?;
        fs::write(service_dir.join("service.yaml"), manifest_yaml).with_context(|| {
            format!(
                "Failed to write migrated managed service manifest in staging: {}",
                service_dir.join("service.yaml").display()
            )
        })?;

        let state_file = CurrentServiceStateFile {
            schema_version: CURRENT_SERVICE_STATE_SCHEMA_VERSION,
            local_service_id: local_service_id.clone(),
            updated_at: legacy_updated_at.clone(),
            desired_state: persisted_service.desired_state.into(),
        };
        let state_bytes = serde_json::to_vec_pretty(&state_file)
            .context("Failed to encode migrated managed service state.json")?;
        fs::write(service_dir.join("state.json"), state_bytes).with_context(|| {
            format!(
                "Failed to write migrated managed service state in staging: {}",
                service_dir.join("state.json").display()
            )
        })?;
    }

    fs::remove_file(&legacy_state_path).with_context(|| {
        format!(
            "Failed to remove legacy services-state.json from staging directory: {}",
            legacy_state_path.display()
        )
    })?;
    Ok(())
}

fn generate_unique_local_service_id(allocated: &mut BTreeSet<String>) -> Result<String> {
    for _ in 0..16 {
        let candidate = format!("svc_{}", Ulid::new().to_string().to_ascii_lowercase());
        if allocated.insert(candidate.clone()) {
            return Ok(candidate);
        }
    }
    bail!("failed to allocate unique local_service_id during migration")
}

fn move_or_create_service_data_dir(old_path: &Path, new_path: &Path) -> Result<()> {
    if new_path.exists() {
        bail!(
            "Refusing to overwrite migrated service data directory in staging: {}",
            new_path.display()
        );
    }

    if old_path.exists() {
        if let Some(parent) = new_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "Failed to create migrated service data parent directory in staging: {}",
                    parent.display()
                )
            })?;
        }
        fs::rename(old_path, new_path).with_context(|| {
            format!(
                "Failed to move legacy service data directory from {} to {}",
                old_path.display(),
                new_path.display()
            )
        })?;
        return Ok(());
    }

    fs::create_dir_all(new_path).with_context(|| {
        format!(
            "Failed to create migrated service data directory in staging: {}",
            new_path.display()
        )
    })
}

fn migrate_legacy_service_manifest(
    manifest: LegacyServiceManifest,
    old_service_data_dir: &Path,
    new_service_appdata_dir: &Path,
    new_service_artifacts_dir: &Path,
) -> Result<CurrentServiceManifestDocument> {
    let runtime = manifest._runtime;
    let source = match manifest.source {
        LegacyServiceSource::Docker { image } => CurrentServiceSource {
            image: Some(image),
            ..CurrentServiceSource::default()
        },
        LegacyServiceSource::WasmtimeFile { component } => CurrentServiceSource {
            file: Some(migrate_wasmtime_component(
                &component,
                old_service_data_dir,
                new_service_appdata_dir,
                new_service_artifacts_dir,
            )?),
            ..CurrentServiceSource::default()
        },
        LegacyServiceSource::WasmtimeUrl { url } => CurrentServiceSource {
            url: Some(url),
            ..CurrentServiceSource::default()
        },
    };
    let expose = manifest.expose;
    let client_kind = expose
        .as_ref()
        .and_then(|expose| expose.usage.as_ref())
        .map(|usage| legacy_usage_to_current_client_kind(usage.kind));
    let client_path = expose
        .as_ref()
        .and_then(|expose| expose.usage.as_ref())
        .and_then(|usage| normalize_optional(usage.path.clone()));
    let client_icon_url = expose
        .as_ref()
        .and_then(|expose| normalize_optional(expose.icon_url.clone()));
    let client = (client_kind.is_some() || client_path.is_some() || client_icon_url.is_some())
        .then_some(CurrentServiceClient {
            kind: client_kind,
            path: client_path,
            icon_url: client_icon_url,
        });
    let publish = manifest
        .ports
        .into_iter()
        .enumerate()
        .map(|(index, port)| -> Result<_> {
            if port.protocol != LegacyServicePortProtocol::Tcp {
                bail!("Legacy UDP service ports cannot be represented by fungi: service/v1");
            }
            let name = port.name.unwrap_or_else(|| {
                if index == 0 {
                    "main".to_string()
                } else {
                    format!("main-{index}")
                }
            });
            let current_port = match runtime {
                LegacyRuntimeKind::Docker => port.service_port,
                LegacyRuntimeKind::Wasmtime => {
                    if port.host_port == 0 {
                        port.service_port
                    } else {
                        port.host_port
                    }
                }
            };
            Ok((
                name,
                CurrentServicePublishEntry {
                    tcp: CurrentServiceTcp { port: current_port },
                    client: client.clone(),
                },
            ))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    if publish.is_empty() {
        bail!(
            "Legacy managed service '{}' has no published TCP ports and cannot be represented by fungi: service/v1",
            manifest.name
        );
    }

    let run = CurrentServiceRun {
        provider: runtime.into(),
        mode: (runtime == LegacyRuntimeKind::Wasmtime && !publish.is_empty())
            .then_some(CurrentServiceRunMode::Http),
        source,
        args: manifest.command,
        env: manifest.env,
        mounts: manifest
            .mounts
            .into_iter()
            .map(|mount| CurrentServiceMount {
                from: rewrite_legacy_managed_manifest_path(mount.host_path, old_service_data_dir),
                to: mount.runtime_path,
            })
            .collect(),
    };

    Ok(CurrentServiceManifestDocument {
        fungi: "service/v1".to_string(),
        id: manifest.name,
        run,
        publish,
    })
}

fn migrate_wasmtime_component(
    component: &Path,
    old_service_data_dir: &Path,
    new_service_appdata_dir: &Path,
    new_service_artifacts_dir: &Path,
) -> Result<String> {
    let Ok(relative_path) = component.strip_prefix(old_service_data_dir) else {
        return Ok(component.display().to_string());
    };
    let staged_component = new_service_appdata_dir.join(relative_path);
    let artifact_component = new_service_artifacts_dir.join("component.wasm");
    fs::create_dir_all(new_service_artifacts_dir).with_context(|| {
        format!(
            "Failed to create migrated service artifacts directory: {}",
            new_service_artifacts_dir.display()
        )
    })?;
    fs::copy(&staged_component, &artifact_component).with_context(|| {
        format!(
            "Failed to migrate Wasmtime component from {} to {}",
            staged_component.display(),
            artifact_component.display()
        )
    })?;
    fs::remove_file(&staged_component).with_context(|| {
        format!(
            "Failed to remove migrated Wasmtime component from appdata: {}",
            staged_component.display()
        )
    })?;
    Ok("$fungi.service.artifacts/component.wasm".to_string())
}

fn legacy_usage_to_current_client_kind(kind: LegacyServiceExposeUsageKind) -> String {
    match kind {
        LegacyServiceExposeUsageKind::Web => "web",
        LegacyServiceExposeUsageKind::Ssh => "ssh",
        LegacyServiceExposeUsageKind::Raw => "raw",
    }
    .to_string()
}

fn rewrite_legacy_managed_manifest_path(path: PathBuf, old_root: &Path) -> String {
    if path == old_root {
        return "$fungi.service.data".to_string();
    }

    if let Ok(suffix) = path.strip_prefix(old_root) {
        let suffix = suffix
            .components()
            .map(|component| component.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        return format!("$fungi.service.data/{suffix}");
    }

    path.display().to_string()
}

fn default_legacy_service_state_schema_version() -> u32 {
    LEGACY_SERVICE_STATE_SCHEMA_VERSION
}

#[derive(Debug, Deserialize)]
struct LegacyServiceStateFile {
    #[serde(default = "default_legacy_service_state_schema_version")]
    schema_version: u32,
    #[serde(default)]
    updated_at: String,
    #[serde(default)]
    services: BTreeMap<String, LegacyPersistedService>,
}

#[derive(Debug, Deserialize)]
struct LegacyPersistedService {
    manifest: LegacyServiceManifest,
    desired_state: LegacyDesiredServiceState,
}

#[derive(Debug, Clone, Deserialize)]
struct LegacyServiceManifest {
    name: String,
    #[serde(rename = "runtime")]
    _runtime: LegacyRuntimeKind,
    source: LegacyServiceSource,
    expose: Option<LegacyServiceExpose>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default)]
    mounts: Vec<LegacyServiceMount>,
    #[serde(default)]
    ports: Vec<LegacyServicePort>,
    #[serde(default)]
    command: Vec<String>,
    #[serde(default)]
    #[serde(rename = "entrypoint")]
    _entrypoint: Vec<String>,
    #[serde(rename = "working_dir")]
    _working_dir: Option<String>,
    #[serde(default)]
    #[serde(rename = "labels")]
    _labels: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum LegacyRuntimeKind {
    Docker,
    Wasmtime,
}

#[derive(Debug, Clone, Deserialize)]
enum LegacyServiceSource {
    Docker { image: String },
    WasmtimeFile { component: PathBuf },
    WasmtimeUrl { url: String },
}

#[derive(Debug, Clone, Deserialize)]
struct LegacyServiceExpose {
    #[serde(rename = "service_id")]
    _service_id: String,
    #[serde(rename = "display_name")]
    _display_name: String,
    #[serde(rename = "transport")]
    _transport: LegacyServiceExposeTransport,
    usage: Option<LegacyServiceExposeUsage>,
    icon_url: Option<String>,
    #[serde(rename = "catalog_id")]
    _catalog_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct LegacyServiceExposeTransport {
    #[serde(rename = "kind")]
    _kind: LegacyServiceExposeTransportKind,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum LegacyServiceExposeTransportKind {
    Tcp,
    Raw,
}

#[derive(Debug, Clone, Deserialize)]
struct LegacyServiceExposeUsage {
    kind: LegacyServiceExposeUsageKind,
    path: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum LegacyServiceExposeUsageKind {
    Web,
    Ssh,
    Raw,
}

#[derive(Debug, Clone, Deserialize)]
struct LegacyServiceMount {
    host_path: PathBuf,
    runtime_path: String,
}

#[derive(Debug, Clone, Deserialize)]
struct LegacyServicePort {
    name: Option<String>,
    host_port: u16,
    service_port: u16,
    protocol: LegacyServicePortProtocol,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum LegacyServicePortProtocol {
    Tcp,
    Udp,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum LegacyDesiredServiceState {
    Running,
    Stopped,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
enum CurrentDesiredServiceState {
    Running,
    Stopped,
}

impl From<LegacyDesiredServiceState> for CurrentDesiredServiceState {
    fn from(value: LegacyDesiredServiceState) -> Self {
        match value {
            LegacyDesiredServiceState::Running => Self::Running,
            LegacyDesiredServiceState::Stopped => Self::Stopped,
        }
    }
}

#[derive(Debug, Serialize)]
struct CurrentServiceStateFile {
    schema_version: u32,
    local_service_id: String,
    updated_at: String,
    desired_state: CurrentDesiredServiceState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CurrentServiceManifestDocument {
    fungi: String,
    id: String,
    run: CurrentServiceRun,
    publish: BTreeMap<String, CurrentServicePublishEntry>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum CurrentServiceProvider {
    Docker,
    Wasmtime,
}

impl From<LegacyRuntimeKind> for CurrentServiceProvider {
    fn from(value: LegacyRuntimeKind) -> Self {
        match value {
            LegacyRuntimeKind::Docker => Self::Docker,
            LegacyRuntimeKind::Wasmtime => Self::Wasmtime,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CurrentServiceRun {
    provider: CurrentServiceProvider,
    #[serde(skip_serializing_if = "Option::is_none")]
    mode: Option<CurrentServiceRunMode>,
    source: CurrentServiceSource,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    args: Vec<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    env: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    mounts: Vec<CurrentServiceMount>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum CurrentServiceRunMode {
    Http,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct CurrentServiceSource {
    #[serde(skip_serializing_if = "Option::is_none")]
    file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    image: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CurrentServiceMount {
    from: String,
    to: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CurrentServicePublishEntry {
    tcp: CurrentServiceTcp,
    #[serde(skip_serializing_if = "Option::is_none")]
    client: Option<CurrentServiceClient>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CurrentServiceTcp {
    port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CurrentServiceClient {
    #[serde(skip_serializing_if = "Option::is_none")]
    kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(rename = "iconUrl")]
    #[serde(skip_serializing_if = "Option::is_none")]
    icon_url: Option<String>,
}
