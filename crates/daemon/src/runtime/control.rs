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
    manifest::{
        ManifestPathRoots, parse_service_manifest_yaml_with_policy,
        service_expose_endpoint_bindings,
    },
    model::*,
    parse_service_manifest_yaml_with_policy_for_service_paths, peek_service_manifest_name,
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

#[derive(Debug, Clone)]
pub struct AppliedService {
    pub instance: ServiceInstance,
    pub previous_manifest: Option<ServiceManifest>,
    pub desired_state: DesiredServiceState,
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
            RuntimeKind::Link => true,
        }
    }

    pub fn update_allowed_host_paths(&self, allowed_host_paths: Vec<PathBuf>) {
        self.wasmtime.update_allowed_host_paths(allowed_host_paths);
    }

    pub async fn pull(&self, manifest: &ServiceManifest) -> Result<ServiceInstance> {
        Ok(self
            .apply_with_local_service_id(manifest, None)
            .await?
            .instance)
    }

    pub async fn apply(&self, manifest: &ServiceManifest) -> Result<AppliedService> {
        self.apply_with_local_service_id(manifest, None).await
    }

    async fn apply_with_local_service_id(
        &self,
        manifest: &ServiceManifest,
        local_service_id: Option<&str>,
    ) -> Result<AppliedService> {
        self.ensure_runtime_enabled(manifest.runtime)?;

        let previous_service = { self.service_state.lock().persisted_service(&manifest.name) };
        let previous_manifest = previous_service
            .as_ref()
            .map(|service| service.manifest.clone());
        let desired_state = previous_service
            .as_ref()
            .map(|service| service.desired_state)
            .unwrap_or(DesiredServiceState::Stopped);
        let previous_runtime = previous_manifest.as_ref().map(|manifest| manifest.runtime);
        let replacing_existing =
            previous_service.is_some() || self.service_index.lock().contains_key(&manifest.name);

        let resolved_local_service_id = if let Some(service) = previous_service.as_ref() {
            if let Some(requested_local_service_id) = local_service_id
                && requested_local_service_id != service.local_service_id
            {
                bail!(
                    "local_service_id mismatch for service '{}': expected '{}', got '{}'",
                    manifest.name,
                    service.local_service_id,
                    requested_local_service_id
                );
            }
            service.local_service_id.clone()
        } else {
            match local_service_id {
                Some(local_service_id) => local_service_id.to_string(),
                None => self
                    .service_state
                    .lock()
                    .preview_local_service_id(&manifest.name)?,
            }
        };

        if let Some(previous_runtime) = previous_runtime {
            if desired_state == DesiredServiceState::Running {
                self.stop_runtime_only(previous_runtime, &manifest.name)
                    .await?;
            }
            self.remove_runtime_only(previous_runtime, &manifest.name, &resolved_local_service_id)
                .await?;
        }

        let instance = match manifest.runtime {
            RuntimeKind::Docker => self.docker_provider()?.pull(manifest).await,
            RuntimeKind::Wasmtime => {
                if replacing_existing {
                    self.wasmtime
                        .replace_with_local_service_id(manifest, &resolved_local_service_id)
                        .await
                } else {
                    self.wasmtime
                        .pull_with_local_service_id(manifest, &resolved_local_service_id)
                        .await
                }
            }
            RuntimeKind::Link => Ok(self.link_instance_from_manifest(manifest, false)),
        }?;

        self.service_index
            .lock()
            .insert(manifest.name.clone(), manifest.runtime);
        self.service_manifests
            .lock()
            .insert(manifest.name.clone(), manifest.clone());
        self.persist_service(manifest, desired_state, Some(&resolved_local_service_id))?;

        let mut instance = enrich_instance_from_manifest(instance, manifest);
        if desired_state == DesiredServiceState::Running {
            self.start(manifest.runtime, &manifest.name).await?;
            instance = self.inspect(manifest.runtime, &manifest.name).await?;
        }

        Ok(AppliedService {
            instance,
            previous_manifest,
            desired_state,
        })
    }

    pub async fn apply_manifest_yaml(
        &self,
        content: &str,
        base_dir: &Path,
        fungi_home: &Path,
        policy: &ManifestResolutionPolicy,
    ) -> Result<AppliedService> {
        let manifest_name = peek_service_manifest_name(content)?;
        let local_service_id = {
            self.service_state
                .lock()
                .preview_local_service_id(&manifest_name)?
        };
        let used_host_ports = self.reserved_host_ports_except(&manifest_name);
        let path_roots = ManifestPathRoots::for_local_service_id(fungi_home, &local_service_id);
        let manifest = parse_service_manifest_yaml_with_policy_for_service_paths(
            content,
            base_dir,
            fungi_home,
            &path_roots,
            policy,
            &used_host_ports,
        )?;
        self.apply_with_local_service_id(&manifest, Some(&local_service_id))
            .await
    }

    pub async fn pull_manifest_yaml(
        &self,
        content: &str,
        base_dir: &Path,
        fungi_home: &Path,
        policy: &ManifestResolutionPolicy,
    ) -> Result<ServiceInstance> {
        Ok(self
            .apply_manifest_yaml(content, base_dir, fungi_home, policy)
            .await?
            .instance)
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
            RuntimeKind::Link => Ok(()),
        }?;
        self.set_desired_state(name, DesiredServiceState::Running)
    }

    pub async fn stop(&self, runtime: RuntimeKind, name: &str) -> Result<()> {
        let _ = self.ensure_runtime_service(runtime, name).await;
        let stop_result = match runtime {
            RuntimeKind::Docker => self.docker_provider()?.stop(name).await,
            RuntimeKind::Wasmtime => self.wasmtime.stop(name).await,
            RuntimeKind::Link => Ok(()),
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
            RuntimeKind::Wasmtime => {
                let local_service_id = self.service_state.lock().local_service_id(name)?;
                self.wasmtime
                    .remove_with_local_service_id(name, &local_service_id)
                    .await
            }
            RuntimeKind::Link => Ok(()),
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
                        host_port: endpoint.host_port,
                        service_port: endpoint.service_port,
                    })
                    .collect(),
                status: instance.status,
            });
        }

        services.sort_by(|left, right| left.service_name.cmp(&right.service_name));
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
            RuntimeKind::Link => {
                let manifest = self
                    .get_service_manifest(name)
                    .ok_or_else(|| anyhow::anyhow!("service not found: {name}"))?;
                let running = self
                    .service_state
                    .lock()
                    .desired_state(name)
                    .is_some_and(|state| state == DesiredServiceState::Running);
                return Ok(self.link_instance_from_manifest(&manifest, running));
            }
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
            RuntimeKind::Link => bail!("link services do not have runtime logs"),
        }
    }

    pub async fn restore_persisted_state(&self) -> Result<()> {
        let persisted_services = { self.service_state.lock().persisted_services() };

        for PersistedService {
            local_service_id,
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
                && let Err(error) = self.wasmtime.restore(&manifest, &local_service_id).await
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

    pub fn desired_running_service_manifests(&self) -> Vec<ServiceManifest> {
        self.service_state
            .lock()
            .persisted_services()
            .into_iter()
            .filter_map(|service| {
                (service.desired_state == DesiredServiceState::Running).then_some(service.manifest)
            })
            .collect()
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
            RuntimeKind::Link => {}
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
        let local_service_id = self.service_state.lock().local_service_id(name)?;
        self.wasmtime.restore(&manifest, &local_service_id).await
    }

    fn persist_service(
        &self,
        manifest: &ServiceManifest,
        desired_state: DesiredServiceState,
        local_service_id: Option<&str>,
    ) -> Result<()> {
        self.service_state
            .lock()
            .upsert_service_with_local_service_id(manifest, desired_state, local_service_id)
            .map(|_| ())
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
        self.reserved_host_ports_except("")
    }

    fn reserved_host_ports_except(&self, service_name: &str) -> BTreeSet<u16> {
        self.service_manifests
            .lock()
            .values()
            .filter(|manifest| manifest.name != service_name)
            .flat_map(|manifest| manifest.ports.iter().map(|port| port.host_port))
            .collect()
    }

    async fn stop_runtime_only(&self, runtime: RuntimeKind, name: &str) -> Result<()> {
        let stop_result = match runtime {
            RuntimeKind::Docker => self.docker_provider()?.stop(name).await,
            RuntimeKind::Wasmtime => self.wasmtime.stop(name).await,
            RuntimeKind::Link => Ok(()),
        };

        match stop_result {
            Ok(()) => Ok(()),
            Err(error)
                if runtime == RuntimeKind::Docker && is_missing_docker_container_error(&error) =>
            {
                log::warn!(
                    "Docker service '{}' is already missing during apply stop; replacing local state only",
                    name
                );
                Ok(())
            }
            Err(error)
                if runtime == RuntimeKind::Wasmtime
                    && error.to_string().contains("wasmtime service not found") =>
            {
                log::warn!(
                    "Wasmtime service '{}' was not running during apply stop: {}",
                    name,
                    error
                );
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    async fn remove_runtime_only(
        &self,
        runtime: RuntimeKind,
        name: &str,
        local_service_id: &str,
    ) -> Result<()> {
        let remove_result = match runtime {
            RuntimeKind::Docker => self.docker_provider()?.remove(name).await,
            RuntimeKind::Wasmtime => {
                self.wasmtime
                    .remove_with_local_service_id(name, local_service_id)
                    .await
            }
            RuntimeKind::Link => Ok(()),
        };

        match remove_result {
            Ok(()) => Ok(()),
            Err(error)
                if runtime == RuntimeKind::Docker && is_missing_docker_container_error(&error) =>
            {
                log::warn!(
                    "Docker service '{}' is already missing during apply remove; replacing local state only",
                    name
                );
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    fn link_instance_from_manifest(
        &self,
        manifest: &ServiceManifest,
        running: bool,
    ) -> ServiceInstance {
        ServiceInstance {
            id: format!("link:{}", manifest.name),
            runtime: RuntimeKind::Link,
            name: manifest.name.clone(),
            source: match &manifest.source {
                ServiceSource::TcpLink { host, port } => format!("{host}:{port}"),
                _ => "link".to_string(),
            },
            labels: manifest.labels.clone(),
            ports: manifest.ports.clone(),
            exposed_endpoints: service_expose_endpoint_bindings(manifest),
            status: ServiceStatus {
                state: if running { "running" } else { "stopped" }.to_string(),
                running,
            },
        }
    }
}
