use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;
use ulid::Ulid;

use crate::runtime::{
    ServiceManifest, parse_managed_service_manifest_yaml, service_manifest_to_yaml,
};

const SERVICE_STATE_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DesiredServiceState {
    Running,
    Stopped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedService {
    pub local_service_id: String,
    pub manifest: ServiceManifest,
    pub desired_state: DesiredServiceState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ServiceStateFile {
    #[serde(default = "default_schema_version")]
    schema_version: u32,
    local_service_id: String,
    #[serde(default)]
    updated_at: String,
    desired_state: DesiredServiceState,
}

impl ServiceStateFile {
    fn default_for_local_service_id(local_service_id: String) -> Self {
        Self {
            schema_version: SERVICE_STATE_SCHEMA_VERSION,
            local_service_id,
            updated_at: String::new(),
            desired_state: DesiredServiceState::Stopped,
        }
    }
}

pub struct ServiceStateStore {
    services_root: PathBuf,
    state: BTreeMap<String, PersistedService>,
    name_index: BTreeMap<String, String>,
}

impl ServiceStateStore {
    pub fn load(services_root: PathBuf) -> Result<Self> {
        fs::create_dir_all(&services_root).with_context(|| {
            format!(
                "Failed to create services directory: {}",
                services_root.display()
            )
        })?;

        let fungi_home = services_root
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let mut state = BTreeMap::new();
        let mut name_index = BTreeMap::new();

        for entry in fs::read_dir(&services_root).with_context(|| {
            format!(
                "Failed to read services directory: {}",
                services_root.display()
            )
        })? {
            let entry = entry?;
            let service_dir = entry.path();
            if !service_dir.is_dir() {
                continue;
            }

            let manifest_path = service_dir.join("service.yaml");
            if !manifest_path.exists() {
                continue;
            }

            let manifest_yaml = fs::read_to_string(&manifest_path).with_context(|| {
                format!(
                    "Failed to read managed service manifest: {}",
                    manifest_path.display()
                )
            })?;
            let state_file =
                load_service_state_file(&service_dir.join("state.json"), &service_dir)?;
            let service_data_dir = fungi_home.join("data").join(&state_file.local_service_id);
            let manifest = parse_managed_service_manifest_yaml(
                &manifest_yaml,
                &service_dir,
                &fungi_home,
                &service_data_dir,
            )
            .with_context(|| {
                format!(
                    "Failed to parse managed service manifest: {}",
                    manifest_path.display()
                )
            })?;

            if let Some(existing_local_service_id) =
                name_index.insert(manifest.name.clone(), state_file.local_service_id.clone())
            {
                bail!(
                    "Duplicate managed service name '{}' for local ids '{}' and '{}'",
                    manifest.name,
                    existing_local_service_id,
                    state_file.local_service_id
                );
            }

            if state
                .insert(
                    state_file.local_service_id.clone(),
                    PersistedService {
                        local_service_id: state_file.local_service_id,
                        manifest,
                        desired_state: state_file.desired_state,
                    },
                )
                .is_some()
            {
                bail!(
                    "Duplicate managed service local id in {}",
                    services_root.display()
                );
            }
        }

        Ok(Self {
            services_root,
            state,
            name_index,
        })
    }

    pub fn persisted_services(&self) -> Vec<PersistedService> {
        self.state.values().cloned().collect()
    }

    pub fn desired_state(&self, service_name: &str) -> Option<DesiredServiceState> {
        let local_service_id = self.name_index.get(service_name)?;
        self.state
            .get(local_service_id)
            .map(|service| service.desired_state)
    }

    pub fn preview_local_service_id(&self, service_name: &str) -> Result<String> {
        if let Some(local_service_id) = self.name_index.get(service_name) {
            return Ok(local_service_id.clone());
        }

        self.generate_unique_local_service_id()
    }

    pub fn upsert_service_with_local_service_id(
        &mut self,
        manifest: &ServiceManifest,
        desired_state: DesiredServiceState,
        requested_local_service_id: Option<&str>,
    ) -> Result<String> {
        let local_service_id =
            self.resolve_upsert_local_service_id(&manifest.name, requested_local_service_id)?;
        self.name_index
            .insert(manifest.name.clone(), local_service_id.clone());
        self.state.insert(
            local_service_id.clone(),
            PersistedService {
                local_service_id: local_service_id.clone(),
                manifest: manifest.clone(),
                desired_state,
            },
        );
        self.save_service(&local_service_id)?;
        Ok(local_service_id)
    }

    pub fn set_desired_state(
        &mut self,
        service_name: &str,
        desired_state: DesiredServiceState,
    ) -> Result<()> {
        let local_service_id = self.lookup_local_service_id(service_name)?;
        let service = self
            .state
            .get_mut(&local_service_id)
            .ok_or_else(|| anyhow::anyhow!("persisted service not found: {service_name}"))?;
        service.desired_state = desired_state;
        self.save_service(&local_service_id)
    }

    pub fn remove_service(&mut self, service_name: &str) -> Result<()> {
        let Some(local_service_id) = self.name_index.remove(service_name) else {
            return Ok(());
        };

        self.state.remove(&local_service_id);
        let service_dir = self.service_dir(&local_service_id);
        if service_dir.exists() {
            fs::remove_dir_all(&service_dir).with_context(|| {
                format!(
                    "Failed to remove managed service directory: {}",
                    service_dir.display()
                )
            })?;
        }
        Ok(())
    }

    fn save_service(&mut self, local_service_id: &str) -> Result<()> {
        let service = self
            .state
            .get(local_service_id)
            .ok_or_else(|| anyhow::anyhow!("persisted service not found: {local_service_id}"))?;
        let service_dir = self.service_dir(local_service_id);
        fs::create_dir_all(&service_dir).with_context(|| {
            format!(
                "Failed to create managed service directory: {}",
                service_dir.display()
            )
        })?;
        self.ensure_service_data_dir(local_service_id)?;

        let manifest_yaml = service_manifest_to_yaml(&service.manifest)?;
        atomic_write(&service_dir.join("service.yaml"), manifest_yaml.as_bytes())?;

        let state_file = ServiceStateFile {
            schema_version: SERVICE_STATE_SCHEMA_VERSION,
            local_service_id: local_service_id.to_string(),
            updated_at: Utc::now().to_rfc3339(),
            desired_state: service.desired_state,
        };
        let state_bytes =
            serde_json::to_vec_pretty(&state_file).context("Failed to encode service state")?;
        atomic_write(&service_dir.join("state.json"), &state_bytes)
    }

    fn service_dir(&self, local_service_id: &str) -> PathBuf {
        self.services_root.join(local_service_id)
    }

    fn service_data_dir(&self, local_service_id: &str) -> PathBuf {
        self.services_root
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("data")
            .join(local_service_id)
    }

    fn ensure_service_data_dir(&self, local_service_id: &str) -> Result<()> {
        let data_dir = self.service_data_dir(local_service_id);
        fs::create_dir_all(&data_dir).with_context(|| {
            format!(
                "Failed to create service data directory: {}",
                data_dir.display()
            )
        })
    }

    fn lookup_local_service_id(&self, service_name: &str) -> Result<String> {
        self.name_index
            .get(service_name)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("persisted service not found: {service_name}"))
    }

    fn resolve_upsert_local_service_id(
        &self,
        service_name: &str,
        requested_local_service_id: Option<&str>,
    ) -> Result<String> {
        let requested_local_service_id = requested_local_service_id
            .map(|value| normalize_local_service_id(value, "local_service_id"))
            .transpose()?;

        if let Some(existing_local_service_id) = self.name_index.get(service_name) {
            if let Some(requested_local_service_id) = requested_local_service_id.as_ref()
                && requested_local_service_id != existing_local_service_id
            {
                bail!(
                    "local_service_id mismatch for service '{}': expected '{}', got '{}'",
                    service_name,
                    existing_local_service_id,
                    requested_local_service_id
                );
            }
            return Ok(existing_local_service_id.clone());
        }

        if let Some(requested_local_service_id) = requested_local_service_id {
            if let Some(existing_service) = self.state.get(&requested_local_service_id)
                && existing_service.manifest.name != service_name
            {
                bail!(
                    "local_service_id '{}' is already assigned to service '{}'",
                    requested_local_service_id,
                    existing_service.manifest.name
                );
            }
            return Ok(requested_local_service_id);
        }

        self.generate_unique_local_service_id()
    }

    fn generate_unique_local_service_id(&self) -> Result<String> {
        for _ in 0..16 {
            let candidate = format!("svc_{}", Ulid::new().to_string().to_ascii_lowercase());
            if !self.state.contains_key(&candidate) {
                return Ok(candidate);
            }
        }

        bail!("failed to allocate unique local_service_id")
    }
}

fn load_service_state_file(path: &Path, service_dir: &Path) -> Result<ServiceStateFile> {
    let local_service_id = local_service_id_from_service_dir(service_dir)?;
    if !path.exists() {
        return Ok(ServiceStateFile::default_for_local_service_id(
            local_service_id,
        ));
    }

    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read service state file: {}", path.display()))?;
    let state: ServiceStateFile = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse service state file: {}", path.display()))?;
    if state.schema_version != SERVICE_STATE_SCHEMA_VERSION {
        bail!(
            "Unsupported service state schema version {} in {}",
            state.schema_version,
            path.display()
        );
    }
    let normalized_local_service_id =
        normalize_local_service_id(&state.local_service_id, "local_service_id")?;
    if normalized_local_service_id != local_service_id {
        bail!(
            "Managed service state local_service_id '{}' does not match directory '{}'",
            normalized_local_service_id,
            local_service_id
        );
    }
    Ok(ServiceStateFile {
        local_service_id: normalized_local_service_id,
        ..state
    })
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).with_context(|| {
        format!(
            "Failed to create service state directory: {}",
            parent.display()
        )
    })?;

    let mut temp = NamedTempFile::new_in(parent).with_context(|| {
        format!(
            "Failed to create temporary service state file in {}",
            parent.display()
        )
    })?;
    temp.write_all(bytes)
        .context("Failed to write temporary service state file")?;
    temp.as_file_mut()
        .sync_all()
        .context("Failed to flush temporary service state file")?;
    temp.persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("Failed to persist service state file: {}", path.display()))?;
    Ok(())
}

fn default_schema_version() -> u32 {
    SERVICE_STATE_SCHEMA_VERSION
}

fn normalize_local_service_id(value: &str, field_name: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("{field_name} must not be empty");
    }
    if !trimmed
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        bail!("{field_name} must contain only ASCII letters, numbers, '-' or '_'");
    }
    Ok(trimmed.to_string())
}

fn local_service_id_from_service_dir(service_dir: &Path) -> Result<String> {
    let value = service_dir
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Failed to determine managed service directory name: {}",
                service_dir.display()
            )
        })?;
    normalize_local_service_id(value, "service directory name")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{RuntimeKind, ServiceSource};

    #[test]
    fn round_trips_persisted_services() {
        let dir = tempfile::TempDir::new().unwrap();
        let services_root = dir.path().join("services");
        let mut store = ServiceStateStore::load(services_root.clone()).unwrap();

        let manifest = ServiceManifest {
            name: "demo".into(),
            runtime: RuntimeKind::Docker,
            source: ServiceSource::Docker {
                image: "nginx:latest".into(),
            },
            expose: None,
            env: BTreeMap::new(),
            mounts: Vec::new(),
            ports: Vec::new(),
            command: Vec::new(),
            entrypoint: Vec::new(),
            working_dir: None,
            labels: BTreeMap::new(),
        };

        store
            .upsert_service_with_local_service_id(&manifest, DesiredServiceState::Stopped, None)
            .unwrap();

        let persisted = store.persisted_services();
        assert_eq!(persisted.len(), 1);
        let local_service_id = persisted[0].local_service_id.clone();
        assert!(local_service_id.starts_with("svc_"));
        assert_ne!(local_service_id, "demo");

        assert!(
            services_root
                .join(&local_service_id)
                .join("service.yaml")
                .is_file()
        );
        assert!(
            services_root
                .join(&local_service_id)
                .join("state.json")
                .is_file()
        );
        assert!(dir.path().join("data").join(&local_service_id).is_dir());

        let reloaded = ServiceStateStore::load(services_root).unwrap();
        let services = reloaded.persisted_services();
        assert_eq!(services.len(), 1);
        assert_eq!(services[0].local_service_id, local_service_id);
        assert_eq!(services[0].manifest.name, "demo");
        assert_eq!(services[0].desired_state, DesiredServiceState::Stopped);
    }

    #[test]
    fn remove_service_preserves_service_data_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        let services_root = dir.path().join("services");
        let mut store = ServiceStateStore::load(services_root.clone()).unwrap();

        let manifest = ServiceManifest {
            name: "demo".into(),
            runtime: RuntimeKind::Docker,
            source: ServiceSource::Docker {
                image: "nginx:latest".into(),
            },
            expose: None,
            env: BTreeMap::new(),
            mounts: Vec::new(),
            ports: Vec::new(),
            command: Vec::new(),
            entrypoint: Vec::new(),
            working_dir: None,
            labels: BTreeMap::new(),
        };

        let local_service_id = store
            .upsert_service_with_local_service_id(&manifest, DesiredServiceState::Stopped, None)
            .unwrap();
        let service_dir = services_root.join(&local_service_id);
        let data_dir = dir.path().join("data").join(&local_service_id);
        let data_file = data_dir.join("keep.txt");
        fs::write(&data_file, b"persist me").unwrap();

        store.remove_service("demo").unwrap();

        assert!(!service_dir.exists());
        assert!(data_dir.is_dir());
        assert_eq!(fs::read_to_string(&data_file).unwrap(), "persist me");
    }
}
