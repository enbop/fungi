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

use crate::runtime::{ServiceManifest, parse_service_manifest_yaml, service_manifest_to_yaml};

const SERVICE_STATE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DesiredServiceState {
    Running,
    Stopped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedService {
    pub manifest: ServiceManifest,
    pub desired_state: DesiredServiceState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ServiceStateFile {
    #[serde(default = "default_schema_version")]
    schema_version: u32,
    #[serde(default)]
    updated_at: String,
    desired_state: DesiredServiceState,
}

impl Default for ServiceStateFile {
    fn default() -> Self {
        Self {
            schema_version: SERVICE_STATE_SCHEMA_VERSION,
            updated_at: String::new(),
            desired_state: DesiredServiceState::Stopped,
        }
    }
}

pub struct ServiceStateStore {
    services_root: PathBuf,
    state: BTreeMap<String, PersistedService>,
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
            let manifest = parse_service_manifest_yaml(&manifest_yaml, &service_dir, &fungi_home)
                .with_context(|| {
                format!(
                    "Failed to parse managed service manifest: {}",
                    manifest_path.display()
                )
            })?;
            let state_file = load_service_state_file(&service_dir.join("state.json"))?;
            state.insert(
                manifest.name.clone(),
                PersistedService {
                    manifest,
                    desired_state: state_file.desired_state,
                },
            );
        }

        Ok(Self {
            services_root,
            state,
        })
    }

    pub fn persisted_services(&self) -> Vec<PersistedService> {
        self.state.values().cloned().collect()
    }

    pub fn desired_state(&self, service_name: &str) -> Option<DesiredServiceState> {
        self.state
            .get(service_name)
            .map(|service| service.desired_state)
    }

    pub fn upsert_service(
        &mut self,
        manifest: &ServiceManifest,
        desired_state: DesiredServiceState,
    ) -> Result<()> {
        self.state.insert(
            manifest.name.clone(),
            PersistedService {
                manifest: manifest.clone(),
                desired_state,
            },
        );
        self.save_service(&manifest.name)
    }

    pub fn set_desired_state(
        &mut self,
        service_name: &str,
        desired_state: DesiredServiceState,
    ) -> Result<()> {
        let service = self
            .state
            .get_mut(service_name)
            .ok_or_else(|| anyhow::anyhow!("persisted service not found: {service_name}"))?;
        service.desired_state = desired_state;
        self.save_service(service_name)
    }

    pub fn remove_service(&mut self, service_name: &str) -> Result<()> {
        self.state.remove(service_name);
        let service_dir = self.service_dir(service_name);
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

    fn save_service(&mut self, service_name: &str) -> Result<()> {
        let service = self
            .state
            .get(service_name)
            .ok_or_else(|| anyhow::anyhow!("persisted service not found: {service_name}"))?;
        let service_dir = self.service_dir(service_name);
        fs::create_dir_all(&service_dir).with_context(|| {
            format!(
                "Failed to create managed service directory: {}",
                service_dir.display()
            )
        })?;
        self.ensure_service_data_dir(service_name)?;

        let manifest_yaml = service_manifest_to_yaml(&service.manifest)?;
        atomic_write(&service_dir.join("service.yaml"), manifest_yaml.as_bytes())?;

        let state_file = ServiceStateFile {
            schema_version: SERVICE_STATE_SCHEMA_VERSION,
            updated_at: Utc::now().to_rfc3339(),
            desired_state: service.desired_state,
        };
        let state_bytes =
            serde_json::to_vec_pretty(&state_file).context("Failed to encode service state")?;
        atomic_write(&service_dir.join("state.json"), &state_bytes)
    }

    fn service_dir(&self, service_name: &str) -> PathBuf {
        self.services_root.join(service_dir_name(service_name))
    }

    fn ensure_service_data_dir(&self, service_name: &str) -> Result<()> {
        let fungi_home = self
            .services_root
            .parent()
            .unwrap_or_else(|| Path::new("."));
        let data_dir = fungi_home
            .join("sandboxes")
            .join(service_dir_name(service_name))
            .join("data");
        fs::create_dir_all(&data_dir).with_context(|| {
            format!(
                "Failed to create service data directory: {}",
                data_dir.display()
            )
        })
    }
}

fn load_service_state_file(path: &Path) -> Result<ServiceStateFile> {
    if !path.exists() {
        return Ok(ServiceStateFile::default());
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
    Ok(state)
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

fn service_dir_name(service_name: &str) -> String {
    let slug = service_name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();

    if slug.is_empty() {
        "service".to_string()
    } else {
        slug
    }
}

fn default_schema_version() -> u32 {
    SERVICE_STATE_SCHEMA_VERSION
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
            .upsert_service(&manifest, DesiredServiceState::Stopped)
            .unwrap();

        assert!(services_root.join("demo/service.yaml").is_file());
        assert!(services_root.join("demo/state.json").is_file());
        assert!(dir.path().join("sandboxes/demo/data").is_dir());

        let reloaded = ServiceStateStore::load(services_root).unwrap();
        let services = reloaded.persisted_services();
        assert_eq!(services.len(), 1);
        assert_eq!(services[0].manifest.name, "demo");
        assert_eq!(services[0].desired_state, DesiredServiceState::Stopped);
    }
}
