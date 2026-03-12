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

use crate::runtime::ServiceManifest;

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
    #[serde(default)]
    services: BTreeMap<String, PersistedService>,
}

impl Default for ServiceStateFile {
    fn default() -> Self {
        Self {
            schema_version: SERVICE_STATE_SCHEMA_VERSION,
            updated_at: String::new(),
            services: BTreeMap::new(),
        }
    }
}

pub struct ServiceStateStore {
    file_path: PathBuf,
    state: ServiceStateFile,
}

impl ServiceStateStore {
    pub fn load(file_path: PathBuf) -> Result<Self> {
        let state = if file_path.exists() {
            let content = fs::read_to_string(&file_path).with_context(|| {
                format!("Failed to read service state file: {}", file_path.display())
            })?;
            let state: ServiceStateFile = serde_json::from_str(&content).with_context(|| {
                format!(
                    "Failed to parse service state file: {}",
                    file_path.display()
                )
            })?;
            if state.schema_version != SERVICE_STATE_SCHEMA_VERSION {
                bail!(
                    "Unsupported service state schema version {} in {}",
                    state.schema_version,
                    file_path.display()
                );
            }
            state
        } else {
            ServiceStateFile::default()
        };

        Ok(Self { file_path, state })
    }

    pub fn persisted_services(&self) -> Vec<PersistedService> {
        self.state.services.values().cloned().collect()
    }

    pub fn upsert_service(
        &mut self,
        manifest: &ServiceManifest,
        desired_state: DesiredServiceState,
    ) -> Result<()> {
        self.state.services.insert(
            manifest.name.clone(),
            PersistedService {
                manifest: manifest.clone(),
                desired_state,
            },
        );
        self.save()
    }

    pub fn set_desired_state(
        &mut self,
        service_name: &str,
        desired_state: DesiredServiceState,
    ) -> Result<()> {
        let service = self
            .state
            .services
            .get_mut(service_name)
            .ok_or_else(|| anyhow::anyhow!("persisted service not found: {service_name}"))?;
        service.desired_state = desired_state;
        self.save()
    }

    pub fn remove_service(&mut self, service_name: &str) -> Result<()> {
        self.state.services.remove(service_name);
        self.save()
    }

    fn save(&mut self) -> Result<()> {
        self.state.schema_version = SERVICE_STATE_SCHEMA_VERSION;
        self.state.updated_at = Utc::now().to_rfc3339();

        let parent = self.file_path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "Failed to create service state directory: {}",
                parent.display()
            )
        })?;

        let bytes =
            serde_json::to_vec_pretty(&self.state).context("Failed to encode service state")?;
        let mut temp = NamedTempFile::new_in(parent).with_context(|| {
            format!(
                "Failed to create temporary service state file in {}",
                parent.display()
            )
        })?;
        temp.write_all(&bytes)
            .context("Failed to write temporary service state file")?;
        temp.as_file_mut()
            .sync_all()
            .context("Failed to flush temporary service state file")?;
        temp.persist(&self.file_path)
            .map_err(|error| error.error)
            .with_context(|| {
                format!(
                    "Failed to persist service state file: {}",
                    self.file_path.display()
                )
            })?;
        Ok(())
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
        let path = dir.path().join("services-state.json");
        let mut store = ServiceStateStore::load(path.clone()).unwrap();

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

        let reloaded = ServiceStateStore::load(path).unwrap();
        let services = reloaded.persisted_services();
        assert_eq!(services.len(), 1);
        assert_eq!(services[0].manifest.name, "demo");
        assert_eq!(services[0].desired_state, DesiredServiceState::Stopped);
    }
}
