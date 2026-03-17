use std::{
    collections::{BTreeSet, HashMap},
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Result, bail};
use parking_lot::Mutex;

use crate::{
    controls::DockerControl,
    service_state::{DesiredServiceState, PersistedService, ServiceStateStore},
};

use super::{
    helpers::{
        enrich_instance_from_manifest, ensure_services_root_exists,
        is_missing_docker_container_error, missing_instance_from_manifest,
    },
    manifest::{parse_service_manifest_yaml_with_policy, service_expose_endpoint_bindings},
    model::*,
    providers::{DockerRuntimeProvider, RuntimeProvider, WasmtimeRuntimeProvider},
};

#[derive(Clone)]
pub struct RuntimeControl {
    docker: Option<DockerRuntimeProvider>,
    wasmtime: WasmtimeRuntimeProvider,
    wasmtime_enabled: bool,
    service_index: Arc<Mutex<HashMap<String, RuntimeKind>>>,
    service_manifests: Arc<Mutex<HashMap<String, ServiceManifest>>>,
    service_state: Arc<Mutex<ServiceStateStore>>,
}

impl RuntimeControl {
    pub fn new(
        runtime_root: PathBuf,
        launcher_path: PathBuf,
        fungi_home: PathBuf,
        docker: Option<DockerControl>,
        service_state_file: PathBuf,
        allowed_host_paths: Vec<PathBuf>,
        wasmtime_enabled: bool,
    ) -> Result<Self> {
        ensure_services_root_exists(&fungi_home)?;
        Ok(Self {
            docker: docker.map(DockerRuntimeProvider::new),
            wasmtime: WasmtimeRuntimeProvider::new(
                runtime_root,
                launcher_path,
                fungi_home,
                allowed_host_paths,
            ),
            wasmtime_enabled,
            service_index: Arc::new(Mutex::new(HashMap::new())),
            service_manifests: Arc::new(Mutex::new(HashMap::new())),
            service_state: Arc::new(Mutex::new(ServiceStateStore::load(service_state_file)?)),
        })
    }

    pub fn with_wasmtime_provider(
        wasmtime: WasmtimeRuntimeProvider,
        docker: Option<DockerControl>,
        service_state_file: PathBuf,
        wasmtime_enabled: bool,
    ) -> Result<Self> {
        Ok(Self {
            docker: docker.map(DockerRuntimeProvider::new),
            wasmtime,
            wasmtime_enabled,
            service_index: Arc::new(Mutex::new(HashMap::new())),
            service_manifests: Arc::new(Mutex::new(HashMap::new())),
            service_state: Arc::new(Mutex::new(ServiceStateStore::load(service_state_file)?)),
        })
    }

    pub fn supports(&self, runtime: RuntimeKind) -> bool {
        match runtime {
            RuntimeKind::Docker => self.docker.is_some(),
            RuntimeKind::Wasmtime => self.wasmtime_enabled,
        }
    }

    pub fn update_allowed_host_paths(&self, allowed_host_paths: Vec<PathBuf>) {
        self.wasmtime.update_allowed_host_paths(allowed_host_paths);
    }

    pub async fn pull(&self, manifest: &ServiceManifest) -> Result<ServiceInstance> {
        self.ensure_runtime_enabled(manifest.runtime)?;
        {
            let services = self.service_index.lock();
            if services.contains_key(&manifest.name) {
                bail!("service already exists: {}", manifest.name);
            }
        }

        let instance = match manifest.runtime {
            RuntimeKind::Docker => self.docker_provider()?.pull(manifest).await,
            RuntimeKind::Wasmtime => self.wasmtime.pull(manifest).await,
        }?;

        self.service_index
            .lock()
            .insert(manifest.name.clone(), manifest.runtime);
        self.service_manifests
            .lock()
            .insert(manifest.name.clone(), manifest.clone());
        self.persist_service(manifest, DesiredServiceState::Stopped)?;
        Ok(enrich_instance_from_manifest(instance, manifest))
    }

    pub async fn pull_manifest_yaml(
        &self,
        content: &str,
        base_dir: &Path,
        fungi_home: &Path,
        policy: &ManifestResolutionPolicy,
    ) -> Result<ServiceInstance> {
        let manifest = self.resolve_manifest_yaml(content, base_dir, fungi_home, policy)?;
        self.pull(&manifest).await
    }

    pub fn resolve_manifest_yaml(
        &self,
        content: &str,
        base_dir: &Path,
        fungi_home: &Path,
        policy: &ManifestResolutionPolicy,
    ) -> Result<ServiceManifest> {
        let used_host_ports = self.reserved_host_ports();
        parse_service_manifest_yaml_with_policy(
            content,
            base_dir,
            fungi_home,
            policy,
            &used_host_ports,
        )
    }

    pub async fn start(&self, runtime: RuntimeKind, name: &str) -> Result<()> {
        self.ensure_runtime_enabled(runtime)?;
        self.ensure_runtime_service(runtime, name).await?;
        match runtime {
            RuntimeKind::Docker => self.docker_provider()?.start(name).await,
            RuntimeKind::Wasmtime => self.wasmtime.start(name).await,
        }?;
        self.set_desired_state(name, DesiredServiceState::Running)
    }

    pub async fn stop(&self, runtime: RuntimeKind, name: &str) -> Result<()> {
        let _ = self.ensure_runtime_service(runtime, name).await;
        let stop_result = match runtime {
            RuntimeKind::Docker => self.docker_provider()?.stop(name).await,
            RuntimeKind::Wasmtime => self.wasmtime.stop(name).await,
        };

        match stop_result {
            Ok(()) => {}
            Err(error)
                if runtime == RuntimeKind::Docker && is_missing_docker_container_error(&error) =>
            {
                log::warn!(
                    "Docker service '{}' is already missing during stop; reconciling local state only",
                    name
                );
            }
            Err(error) => return Err(error),
        }

        self.set_desired_state(name, DesiredServiceState::Stopped)
    }

    pub async fn remove(&self, runtime: RuntimeKind, name: &str) -> Result<()> {
        let remove_result = match runtime {
            RuntimeKind::Docker => self.docker_provider()?.remove(name).await,
            RuntimeKind::Wasmtime => self.wasmtime.remove(name).await,
        };

        match remove_result {
            Ok(()) => {}
            Err(error)
                if runtime == RuntimeKind::Docker && is_missing_docker_container_error(&error) =>
            {
                log::warn!(
                    "Docker service '{}' is already missing during remove; cleaning up local state only",
                    name
                );
            }
            Err(error) => return Err(error),
        }

        self.service_index.lock().remove(name);
        self.service_manifests.lock().remove(name);
        self.service_state.lock().remove_service(name)?;
        Ok(())
    }

    pub async fn start_by_name(&self, name: &str) -> Result<()> {
        let runtime = self.resolve_runtime(name)?;
        self.start(runtime, name).await
    }

    pub fn get_service_manifest(&self, name: &str) -> Option<ServiceManifest> {
        self.service_manifests.lock().get(name).cloned()
    }

    pub async fn stop_by_name(&self, name: &str) -> Result<()> {
        let runtime = self.resolve_runtime(name)?;
        self.stop(runtime, name).await
    }

    pub async fn remove_by_name(&self, name: &str) -> Result<()> {
        let runtime = self.resolve_runtime(name)?;
        self.remove(runtime, name).await
    }

    pub async fn inspect_by_name(&self, name: &str) -> Result<ServiceInstance> {
        let runtime = self.resolve_runtime(name)?;
        self.inspect(runtime, name).await
    }

    pub async fn logs_by_name(
        &self,
        name: &str,
        options: &ServiceLogsOptions,
    ) -> Result<ServiceLogs> {
        let runtime = self.resolve_runtime(name)?;
        self.logs(runtime, name, options).await
    }

    pub async fn list_catalog_services(&self) -> Result<Vec<CatalogService>> {
        let manifests = self
            .service_manifests
            .lock()
            .values()
            .cloned()
            .collect::<Vec<_>>();

        let mut services = Vec::new();
        for manifest in manifests {
            let Some(expose) = manifest.expose.clone() else {
                continue;
            };

            let instance = match self.inspect(manifest.runtime, &manifest.name).await {
                Ok(instance) => instance,
                Err(error) => {
                    log::warn!(
                        "Failed to inspect service '{}' for discovery: {}",
                        manifest.name,
                        error
                    );
                    continue;
                }
            };

            if !instance.status.running {
                continue;
            }

            services.push(CatalogService {
                service_name: manifest.name.clone(),
                service_id: expose.service_id,
                display_name: expose.display_name,
                runtime: manifest.runtime,
                transport: expose.transport,
                usage: expose.usage,
                icon_url: expose.icon_url,
                catalog_id: expose.catalog_id,
                endpoints: service_expose_endpoint_bindings(&manifest)
                    .into_iter()
                    .map(|endpoint| CatalogServiceEndpoint {
                        name: endpoint.name,
                        protocol: endpoint.protocol,
                        service_port: endpoint.service_port,
                    })
                    .collect(),
                status: instance.status,
            });
        }

        services.sort_by(|left, right| left.service_id.cmp(&right.service_id));
        Ok(services)
    }

    pub async fn list_services(&self) -> Result<Vec<ServiceInstance>> {
        let manifests = self
            .service_manifests
            .lock()
            .values()
            .cloned()
            .collect::<Vec<_>>();

        let mut services = Vec::new();
        for manifest in manifests {
            let instance = match self.inspect(manifest.runtime, &manifest.name).await {
                Ok(instance) => instance,
                Err(error) => {
                    log::warn!(
                        "Failed to inspect service '{}' during list: {}",
                        manifest.name,
                        error
                    );
                    missing_instance_from_manifest(&manifest)
                }
            };
            services.push(enrich_instance_from_manifest(instance, &manifest));
        }

        services.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(services)
    }

    pub async fn inspect(&self, runtime: RuntimeKind, name: &str) -> Result<ServiceInstance> {
        if let Err(error) = self.ensure_runtime_service(runtime, name).await {
            if let Some(manifest) = self.get_service_manifest(name) {
                log::warn!(
                    "Failed to restore service '{}' for inspect: {}",
                    name,
                    error
                );
                return Ok(missing_instance_from_manifest(&manifest));
            }
            return Err(error);
        }

        let inspect_result = match runtime {
            RuntimeKind::Docker => self.docker_provider()?.inspect(name).await,
            RuntimeKind::Wasmtime => self.wasmtime.inspect(name).await,
        };

        let instance = match inspect_result {
            Ok(instance) => instance,
            Err(error)
                if runtime == RuntimeKind::Docker && is_missing_docker_container_error(&error) =>
            {
                if let Some(manifest) = self.get_service_manifest(name) {
                    log::warn!(
                        "Docker service '{}' is missing during inspect; reporting missing instance",
                        name
                    );
                    return Ok(missing_instance_from_manifest(&manifest));
                }
                return Err(error);
            }
            Err(error) => return Err(error),
        };

        if let Some(manifest) = self.get_service_manifest(name) {
            Ok(enrich_instance_from_manifest(instance, &manifest))
        } else {
            Ok(instance)
        }
    }

    pub async fn logs(
        &self,
        runtime: RuntimeKind,
        name: &str,
        options: &ServiceLogsOptions,
    ) -> Result<ServiceLogs> {
        self.ensure_runtime_enabled(runtime)?;
        self.ensure_runtime_service(runtime, name).await?;
        match runtime {
            RuntimeKind::Docker => self.docker_provider()?.logs(name, options).await,
            RuntimeKind::Wasmtime => self.wasmtime.logs(name, options).await,
        }
    }

    pub async fn restore_persisted_state(&self) -> Result<()> {
        let persisted_services = { self.service_state.lock().persisted_services() };

        for PersistedService {
            manifest,
            desired_state,
        } in persisted_services
        {
            self.service_index
                .lock()
                .insert(manifest.name.clone(), manifest.runtime);
            self.service_manifests
                .lock()
                .insert(manifest.name.clone(), manifest.clone());

            if manifest.runtime == RuntimeKind::Wasmtime
                && self.wasmtime_enabled
                && let Err(error) = self.wasmtime.restore(&manifest).await
            {
                log::warn!(
                    "Failed to restore persisted wasmtime service '{}': {}",
                    manifest.name,
                    error
                );
            }

            if desired_state == DesiredServiceState::Running
                && let Err(error) = self.start(manifest.runtime, &manifest.name).await
            {
                log::warn!(
                    "Failed to reconcile persisted service '{}' to running: {}",
                    manifest.name,
                    error
                );
            }
        }

        Ok(())
    }

    fn docker_provider(&self) -> Result<&DockerRuntimeProvider> {
        self.docker
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("docker runtime is not enabled in config"))
    }

    fn ensure_runtime_enabled(&self, runtime: RuntimeKind) -> Result<()> {
        match runtime {
            RuntimeKind::Docker => {
                if self.docker.is_none() {
                    bail!("docker runtime is not available");
                }
            }
            RuntimeKind::Wasmtime => {
                if !self.wasmtime_enabled {
                    bail!("wasmtime runtime is disabled in config");
                }
            }
        }
        Ok(())
    }

    async fn ensure_runtime_service(&self, runtime: RuntimeKind, name: &str) -> Result<()> {
        if runtime != RuntimeKind::Wasmtime || self.wasmtime.has_service(name) {
            return Ok(());
        }

        let manifest = self
            .service_manifests
            .lock()
            .get(name)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("service not found: {name}"))?;
        self.wasmtime.restore(&manifest).await
    }

    fn persist_service(
        &self,
        manifest: &ServiceManifest,
        desired_state: DesiredServiceState,
    ) -> Result<()> {
        self.service_state
            .lock()
            .upsert_service(manifest, desired_state)
    }

    fn set_desired_state(&self, name: &str, desired_state: DesiredServiceState) -> Result<()> {
        self.service_state
            .lock()
            .set_desired_state(name, desired_state)
    }

    fn resolve_runtime(&self, name: &str) -> Result<RuntimeKind> {
        self.service_index
            .lock()
            .get(name)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("service not found: {name}"))
    }

    fn reserved_host_ports(&self) -> BTreeSet<u16> {
        self.service_manifests
            .lock()
            .values()
            .flat_map(|manifest| manifest.ports.iter().map(|port| port.host_port))
            .collect()
    }
}
