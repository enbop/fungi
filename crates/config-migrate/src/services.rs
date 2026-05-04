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
    CURRENT_SERVICE_STATE_SCHEMA_VERSION, DATA_ROOT_DIR, LEGACY_SERVICE_STATE_FILE,
    LEGACY_SERVICE_STATE_SCHEMA_VERSION, SERVICES_ROOT_DIR, normalize_non_empty,
    normalize_optional,
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
    let data_root = staging_root.join(DATA_ROOT_DIR);
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
        let new_service_data_dir = data_root.join(&local_service_id);
        let new_live_service_data_dir = live_root.join(DATA_ROOT_DIR).join(&local_service_id);
        move_or_create_service_data_dir(&old_service_data_dir, &new_service_data_dir)?;

        let manifest_document = migrate_legacy_service_manifest(
            persisted_service.manifest,
            &old_live_service_data_dir,
            &new_live_service_data_dir,
        );
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
    new_service_data_dir: &Path,
) -> CurrentServiceManifestDocument {
    let source = match manifest.source {
        LegacyServiceSource::Docker { image } => CurrentServiceManifestSource {
            image: Some(image),
            ..CurrentServiceManifestSource::default()
        },
        LegacyServiceSource::WasmtimeFile { component } => CurrentServiceManifestSource {
            file: Some(
                rewrite_legacy_managed_path(component, old_service_data_dir, new_service_data_dir)
                    .display()
                    .to_string(),
            ),
            ..CurrentServiceManifestSource::default()
        },
        LegacyServiceSource::WasmtimeUrl { url } => CurrentServiceManifestSource {
            url: Some(url),
            ..CurrentServiceManifestSource::default()
        },
    };

    CurrentServiceManifestDocument {
        api_version: "fungi.rs/v1alpha1".to_string(),
        kind: "ServiceManifest".to_string(),
        metadata: CurrentServiceManifestMetadata {
            name: manifest.name,
            labels: manifest.labels,
        },
        spec: CurrentServiceManifestSpec {
            runtime: manifest.runtime,
            source,
            expose: manifest.expose.map(|expose| CurrentServiceManifestExpose {
                enabled: true,
                transport: Some(CurrentServiceManifestExposeTransport {
                    kind: expose.transport.kind,
                }),
                usage: expose.usage.map(|usage| CurrentServiceManifestExposeUsage {
                    kind: usage.kind,
                    path: normalize_optional(usage.path),
                }),
                icon_url: normalize_optional(expose.icon_url),
                catalog_id: normalize_optional(expose.catalog_id),
            }),
            env: manifest.env,
            mounts: manifest
                .mounts
                .into_iter()
                .map(|mount| CurrentServiceManifestMount {
                    host_path: rewrite_legacy_managed_path(
                        mount.host_path,
                        old_service_data_dir,
                        new_service_data_dir,
                    )
                    .display()
                    .to_string(),
                    runtime_path: mount.runtime_path,
                })
                .collect(),
            ports: manifest
                .ports
                .into_iter()
                .map(|port| CurrentServiceManifestPort {
                    host_port: Some(CurrentServiceManifestHostPort::Fixed(port.host_port)),
                    service_port: port.service_port,
                    name: port.name,
                    protocol: port.protocol,
                })
                .collect(),
            command: manifest.command,
            entrypoint: manifest.entrypoint,
            working_dir: manifest.working_dir.map(|path| {
                rewrite_legacy_managed_path(
                    PathBuf::from(path),
                    old_service_data_dir,
                    new_service_data_dir,
                )
                .display()
                .to_string()
            }),
        },
    }
}

fn rewrite_legacy_managed_path(path: PathBuf, old_root: &Path, new_root: &Path) -> PathBuf {
    if path == old_root {
        return new_root.to_path_buf();
    }

    if let Ok(suffix) = path.strip_prefix(old_root) {
        return new_root.join(suffix);
    }

    path
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
    runtime: LegacyRuntimeKind,
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
    entrypoint: Vec<String>,
    working_dir: Option<String>,
    #[serde(default)]
    labels: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
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
    transport: LegacyServiceExposeTransport,
    usage: Option<LegacyServiceExposeUsage>,
    icon_url: Option<String>,
    catalog_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct LegacyServiceExposeTransport {
    kind: LegacyServiceExposeTransportKind,
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

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
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
    #[serde(rename = "apiVersion")]
    api_version: String,
    kind: String,
    metadata: CurrentServiceManifestMetadata,
    spec: CurrentServiceManifestSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CurrentServiceManifestMetadata {
    name: String,
    #[serde(default)]
    labels: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CurrentServiceManifestSpec {
    runtime: LegacyRuntimeKind,
    source: CurrentServiceManifestSource,
    expose: Option<CurrentServiceManifestExpose>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default)]
    mounts: Vec<CurrentServiceManifestMount>,
    #[serde(default)]
    ports: Vec<CurrentServiceManifestPort>,
    #[serde(default)]
    command: Vec<String>,
    #[serde(default)]
    entrypoint: Vec<String>,
    #[serde(rename = "workingDir")]
    working_dir: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct CurrentServiceManifestSource {
    #[serde(skip_serializing_if = "Option::is_none")]
    image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CurrentServiceManifestMount {
    #[serde(rename = "hostPath")]
    host_path: String,
    #[serde(rename = "runtimePath")]
    runtime_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CurrentServiceManifestPort {
    #[serde(rename = "hostPort")]
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    host_port: Option<CurrentServiceManifestHostPort>,
    #[serde(rename = "servicePort")]
    service_port: u16,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    protocol: LegacyServicePortProtocol,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum CurrentServiceManifestHostPort {
    Fixed(u16),
    Keyword(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CurrentServiceManifestExpose {
    #[serde(default)]
    enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    transport: Option<CurrentServiceManifestExposeTransport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    usage: Option<CurrentServiceManifestExposeUsage>,
    #[serde(rename = "iconUrl")]
    #[serde(skip_serializing_if = "Option::is_none")]
    icon_url: Option<String>,
    #[serde(rename = "catalogId")]
    #[serde(skip_serializing_if = "Option::is_none")]
    catalog_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CurrentServiceManifestExposeTransport {
    kind: LegacyServiceExposeTransportKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CurrentServiceManifestExposeUsage {
    kind: LegacyServiceExposeUsageKind,
    path: Option<String>,
}
