use std::{
    collections::{BTreeSet, HashMap},
    path::{Path, PathBuf},
    sync::{Arc, Weak},
};

use anyhow::{Result, bail};
use parking_lot::Mutex;

use crate::service_state::{DesiredServiceState, PersistedService, ServiceStateStore};

use super::{
    helpers::{
        enrich_instance_from_manifest, ensure_services_root_exists, missing_instance_from_manifest,
    },
    manifest::{
        ManifestPathRoots, parse_service_manifest_yaml_with_policy,
        service_expose_endpoint_bindings,
    },
    model::*,
    parse_service_manifest_yaml_with_policy_for_service_paths, peek_service_manifest_name,
    providers::WasmtimeRuntimeProvider,
};

#[derive(Clone)]
pub struct RuntimeControl {
    wasmtime: WasmtimeRuntimeProvider,
    wasmtime_enabled: bool,
    service_index: Arc<Mutex<HashMap<String, RuntimeKind>>>,
    service_manifests: Arc<Mutex<HashMap<String, ServiceManifest>>>,
    service_state: Arc<Mutex<ServiceStateStore>>,
    service_operations: Arc<Mutex<HashMap<String, Weak<tokio::sync::Mutex<()>>>>>,
    pending_cleanup: Arc<Mutex<HashMap<String, PendingRuntimeCleanup>>>,
}

#[derive(Clone)]
struct PendingRuntimeCleanup {
    local_service_id: String,
    instance: ServiceInstance,
    apply_error: String,
}

#[derive(Debug, Clone)]
pub struct AppliedService {
    pub instance: ServiceInstance,
    pub previous_manifest: Option<ServiceManifest>,
    pub desired_state: DesiredServiceState,
    pub outcome: ServiceApplyOutcome,
}

impl RuntimeControl {
    pub fn new(
        runtime_root: PathBuf,
        launcher_path: PathBuf,
        fungi_home: PathBuf,
        service_state_file: PathBuf,
        allowed_host_paths: Vec<PathBuf>,
        wasmtime_enabled: bool,
    ) -> Result<Self> {
        ensure_services_root_exists(&fungi_home)?;
        Ok(Self {
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
            service_operations: Arc::default(),
            pending_cleanup: Arc::default(),
        })
    }

    pub fn with_wasmtime_provider(
        wasmtime: WasmtimeRuntimeProvider,
        service_state_file: PathBuf,
        wasmtime_enabled: bool,
    ) -> Result<Self> {
        Ok(Self {
            wasmtime,
            wasmtime_enabled,
            service_index: Arc::new(Mutex::new(HashMap::new())),
            service_manifests: Arc::new(Mutex::new(HashMap::new())),
            service_state: Arc::new(Mutex::new(ServiceStateStore::load(service_state_file)?)),
            service_operations: Arc::default(),
            pending_cleanup: Arc::default(),
        })
    }

    pub fn supports(&self, runtime: RuntimeKind) -> bool {
        match runtime {
            RuntimeKind::Unknown => false,
            RuntimeKind::Wasmtime => self.wasmtime_enabled,
            RuntimeKind::External => true,
        }
    }

    pub fn update_allowed_host_paths(&self, allowed_host_paths: Vec<PathBuf>) {
        self.wasmtime.update_allowed_host_paths(allowed_host_paths);
    }

    pub async fn pull(&self, manifest: &ServiceManifest) -> Result<ServiceInstance> {
        let applied = self.apply(manifest).await?;
        if let Some(message) = applied.outcome.failure_summary() {
            bail!(message);
        }
        Ok(applied.instance)
    }

    pub async fn apply(&self, manifest: &ServiceManifest) -> Result<AppliedService> {
        let _operation = self.lock_service_operation(&manifest.name).await;
        self.apply_with_local_service_id(manifest, None).await
    }

    #[cfg(test)]
    pub(crate) fn seed_in_memory_service_for_test(&self, manifest: ServiceManifest) {
        self.service_index
            .lock()
            .insert(manifest.name.clone(), manifest.runtime);
        self.service_manifests
            .lock()
            .insert(manifest.name.clone(), manifest);
    }

    async fn apply_with_local_service_id(
        &self,
        manifest: &ServiceManifest,
        local_service_id: Option<&str>,
    ) -> Result<AppliedService> {
        self.ensure_runtime_enabled(manifest.runtime)?;
        self.retry_pending_cleanup(&manifest.name).await?;

        let previous_service = { self.service_state.lock().persisted_service(&manifest.name) };
        let failed_service = { self.service_state.lock().failed_service(&manifest.name) };
        if let Some(failed) = &failed_service {
            ensure_matching_definition_id(
                &manifest.name,
                failed.definition_id.as_deref(),
                manifest.definition_id.as_deref(),
            )?;
        }
        let in_memory_manifest = self.service_manifests.lock().get(&manifest.name).cloned();
        let in_memory_runtime = self.service_index.lock().get(&manifest.name).copied();
        let previous_manifest = previous_service
            .as_ref()
            .map(|service| service.manifest.clone())
            .or(in_memory_manifest);
        let desired_state = previous_service
            .as_ref()
            .map(|service| service.desired_state)
            .unwrap_or(DesiredServiceState::Stopped);
        let previous_runtime = previous_manifest
            .as_ref()
            .map(|manifest| manifest.runtime)
            .or(in_memory_runtime);
        let replacing_existing = previous_service.is_some()
            || previous_manifest.is_some()
            || previous_runtime.is_some()
            || failed_service.is_some();
        let manifest_change = match previous_manifest.as_ref() {
            None => ServiceManifestChange::Created,
            Some(previous) if previous == manifest => ServiceManifestChange::Unchanged,
            Some(_) => ServiceManifestChange::Changed,
        };

        if let Some(previous_manifest) = previous_manifest.as_ref() {
            ensure_definition_id_compatible(previous_manifest, manifest)?;
        }

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
            RuntimeKind::Unknown => bail!("unknown service runtime"),
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
            RuntimeKind::External => Ok(self.external_instance_from_manifest(manifest, false)),
        }?;

        if let Err(error) =
            self.persist_service(manifest, desired_state, Some(&resolved_local_service_id))
        {
            let mut failed = enrich_instance_from_manifest(instance, manifest);
            failed.status =
                ServiceStatus::unknown().with_detail(format!("apply persistence error: {error:#}"));
            self.pending_cleanup.lock().insert(
                manifest.name.clone(),
                PendingRuntimeCleanup {
                    local_service_id: resolved_local_service_id,
                    instance: failed,
                    apply_error: format!("{error:#}"),
                },
            );
            return match self.retry_pending_cleanup(&manifest.name).await {
                Ok(()) => Err(error),
                Err(cleanup_error) => {
                    Err(error.context(format!("Runtime rollback also failed: {cleanup_error:#}")))
                }
            };
        }
        self.service_index
            .lock()
            .insert(manifest.name.clone(), manifest.runtime);
        self.service_manifests
            .lock()
            .insert(manifest.name.clone(), manifest.clone());

        let mut instance = enrich_instance_from_manifest(instance, manifest);
        let mut workload_action = ServiceWorkloadAction::None;
        let mut failure = None;
        if desired_state == DesiredServiceState::Running {
            match self.start_locked(manifest.runtime, &manifest.name).await {
                Ok(()) => {
                    if manifest.runtime != RuntimeKind::External {
                        workload_action = ServiceWorkloadAction::Restarted;
                    }
                    match self.inspect_locked(manifest.runtime, &manifest.name).await {
                        Ok(inspected) => instance = inspected,
                        Err(error) => {
                            instance.status = ServiceStatus::unknown();
                            failure = Some(ServiceApplyFailure {
                                stage: ServiceApplyFailureStage::FinalInspection,
                                message: error.to_string(),
                            });
                        }
                    }
                }
                Err(error) => {
                    let mut message = error.to_string();
                    match self.inspect_locked(manifest.runtime, &manifest.name).await {
                        Ok(inspected) => {
                            if inspected.status.is_running()
                                && manifest.runtime != RuntimeKind::External
                            {
                                workload_action = ServiceWorkloadAction::Restarted;
                            }
                            instance = inspected;
                        }
                        Err(inspect_error) => {
                            instance.status = ServiceStatus::unknown();
                            message.push_str(&format!(
                                "; failed to inspect final service state: {inspect_error}"
                            ));
                        }
                    }
                    failure = Some(ServiceApplyFailure {
                        stage: ServiceApplyFailureStage::Restart,
                        message,
                    });
                }
            }
        }

        Ok(AppliedService {
            outcome: ServiceApplyOutcome {
                manifest_change,
                workload_action,
                final_status: instance.status.clone(),
                failure,
            },
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
        let _operation = self.lock_service_operation(&manifest_name).await;
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
        let applied = self
            .apply_manifest_yaml(content, base_dir, fungi_home, policy)
            .await?;
        if let Some(message) = applied.outcome.failure_summary() {
            bail!(message);
        }
        Ok(applied.instance)
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
        let _operation = self.lock_service_operation(name).await;
        self.start_locked(runtime, name).await
    }

    async fn start_locked(&self, runtime: RuntimeKind, name: &str) -> Result<()> {
        self.ensure_service_configuration_loaded(name)?;
        self.ensure_runtime_enabled(runtime)?;
        self.ensure_runtime_service(runtime, name).await?;
        match runtime {
            RuntimeKind::Unknown => bail!("unknown service runtime"),
            RuntimeKind::Wasmtime => self.wasmtime.start(name).await,
            RuntimeKind::External => Ok(()),
        }?;
        self.set_desired_state(name, DesiredServiceState::Running)
    }

    pub async fn stop(&self, runtime: RuntimeKind, name: &str) -> Result<()> {
        let _operation = self.lock_service_operation(name).await;
        self.stop_locked(runtime, name).await
    }

    async fn stop_locked(&self, runtime: RuntimeKind, name: &str) -> Result<()> {
        self.ensure_service_configuration_loaded(name)?;
        let _ = self.ensure_runtime_service(runtime, name).await;
        let stop_result = match runtime {
            RuntimeKind::Unknown => bail!("unknown service runtime"),
            RuntimeKind::Wasmtime => self.wasmtime.stop(name).await,
            RuntimeKind::External => Ok(()),
        };

        stop_result?;

        self.set_desired_state(name, DesiredServiceState::Stopped)
    }

    pub async fn remove(&self, runtime: RuntimeKind, name: &str) -> Result<()> {
        let _operation = self.lock_service_operation(name).await;
        self.remove_locked(runtime, name).await
    }

    async fn remove_locked(&self, runtime: RuntimeKind, name: &str) -> Result<()> {
        let had_pending_cleanup = self.pending_cleanup.lock().contains_key(name);
        self.retry_pending_cleanup(name).await?;
        if self.service_state.lock().failed_service(name).is_some() {
            return self.service_state.lock().remove_service(name);
        }
        if had_pending_cleanup && self.service_state.lock().persisted_service(name).is_none() {
            self.service_index.lock().remove(name);
            self.service_manifests.lock().remove(name);
            return Ok(());
        }
        let remove_result = match runtime {
            RuntimeKind::Unknown => bail!("unknown service runtime"),
            RuntimeKind::Wasmtime => {
                let local_service_id = self.service_state.lock().local_service_id(name)?;
                self.wasmtime
                    .remove_with_local_service_id(name, &local_service_id)
                    .await
            }
            RuntimeKind::External => Ok(()),
        };

        remove_result?;

        self.service_index.lock().remove(name);
        self.service_manifests.lock().remove(name);
        self.service_state.lock().remove_service(name)?;
        Ok(())
    }

    pub async fn start_by_name(&self, name: &str) -> Result<()> {
        let _operation = self.lock_service_operation(name).await;
        let runtime = self.resolve_runtime(name)?;
        self.start_locked(runtime, name).await
    }

    pub fn get_service_manifest(&self, name: &str) -> Option<ServiceManifest> {
        self.service_manifests.lock().get(name).cloned()
    }

    pub async fn stop_by_name(&self, name: &str) -> Result<()> {
        let _operation = self.lock_service_operation(name).await;
        let runtime = self.resolve_runtime(name)?;
        self.stop_locked(runtime, name).await
    }

    pub async fn remove_by_name(&self, name: &str) -> Result<()> {
        let _operation = self.lock_service_operation(name).await;
        let runtime = self.resolve_runtime(name)?;
        self.remove_locked(runtime, name).await
    }

    pub async fn inspect_by_name(&self, name: &str) -> Result<ServiceInstance> {
        let _operation = self.lock_service_operation(name).await;
        let runtime = self.resolve_runtime(name)?;
        self.inspect_locked(runtime, name).await
    }

    pub async fn logs_by_name(
        &self,
        name: &str,
        options: &ServiceLogsOptions,
    ) -> Result<ServiceLogs> {
        let _operation = self.lock_service_operation(name).await;
        let runtime = self.resolve_runtime(name)?;
        self.logs_locked(runtime, name, options).await
    }

    pub(crate) async fn logs_text_by_name_bounded(
        &self,
        name: &str,
        tail: usize,
        max_bytes: usize,
    ) -> Result<BoundedLogText> {
        let _operation = self.lock_service_operation(name).await;
        let runtime = self.resolve_runtime(name)?;
        self.ensure_service_configuration_loaded(name)?;
        self.ensure_runtime_enabled(runtime)?;
        self.ensure_runtime_service(runtime, name).await?;
        match runtime {
            RuntimeKind::Wasmtime => self.wasmtime.logs_text_bounded(name, tail, max_bytes),
            RuntimeKind::External => bail!("external TCP services do not have runtime logs"),
            RuntimeKind::Unknown => bail!("unknown service runtime"),
        }
    }

    pub async fn list_published_device_services(&self) -> Result<Vec<DeviceService>> {
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

            if !instance.status.is_running() {
                continue;
            }

            services.push(DeviceService {
                name: manifest.name.clone(),
                runtime: manifest.runtime,
                metadata: crate::DeviceServiceMetadata {
                    usage: expose.usage,
                    icon_url: expose.icon_url,
                },
                endpoints: service_expose_endpoint_bindings(&manifest)
                    .into_iter()
                    .map(|endpoint| DeviceServiceEndpoint {
                        name: endpoint.name,
                        protocol: endpoint.protocol,
                    })
                    .collect(),
                status: instance.status,
            });
        }

        services.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(services)
    }

    pub async fn list_services(&self) -> Result<Vec<ServiceInstance>> {
        let manifests = self
            .service_manifests
            .lock()
            .values()
            .cloned()
            .collect::<Vec<_>>();

        let mut services = self.service_state.lock().failed_services();
        for manifest in manifests {
            let instance = match self.inspect(manifest.runtime, &manifest.name).await {
                Ok(instance) => instance,
                Err(error) => {
                    log::warn!(
                        "Failed to inspect service '{}' during list: {}",
                        manifest.name,
                        error
                    );
                    let mut instance = missing_instance_from_manifest(&manifest);
                    instance.status =
                        ServiceStatus::unknown().with_detail(format!("runtime error: {error:#}"));
                    instance
                }
            };
            services.push(enrich_instance_from_manifest(instance, &manifest));
        }

        for pending in self.pending_cleanup.lock().values() {
            services.retain(|service| service.name != pending.instance.name);
            services.push(pending.instance.clone());
        }

        services.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(services)
    }

    pub async fn inspect(&self, runtime: RuntimeKind, name: &str) -> Result<ServiceInstance> {
        let _operation = self.lock_service_operation(name).await;
        self.inspect_locked(runtime, name).await
    }

    async fn inspect_locked(&self, runtime: RuntimeKind, name: &str) -> Result<ServiceInstance> {
        if let Some(pending) = self.pending_cleanup.lock().get(name) {
            return Ok(pending.instance.clone());
        }
        if let Some(failed) = self.service_state.lock().failed_service(name) {
            return Ok(failed);
        }
        if let Err(error) = self.ensure_runtime_service(runtime, name).await {
            if let Some(manifest) = self.get_service_manifest(name) {
                log::warn!(
                    "Failed to restore service '{}' for inspect: {}",
                    name,
                    error
                );
                let mut instance = missing_instance_from_manifest(&manifest);
                instance.status =
                    ServiceStatus::unknown().with_detail(format!("restore error: {error:#}"));
                return Ok(instance);
            }
            return Err(error);
        }

        let inspect_result = match runtime {
            RuntimeKind::Unknown => bail!("unknown service runtime"),
            RuntimeKind::Wasmtime => self.wasmtime.inspect(name).await,
            RuntimeKind::External => {
                let manifest = self
                    .get_service_manifest(name)
                    .ok_or_else(|| anyhow::anyhow!("service not found: {name}"))?;
                let running = self
                    .service_state
                    .lock()
                    .desired_state(name)
                    .is_some_and(|state| state == DesiredServiceState::Running);
                return Ok(self.external_instance_from_manifest(&manifest, running));
            }
        };

        let instance = inspect_result?;

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
        let _operation = self.lock_service_operation(name).await;
        self.logs_locked(runtime, name, options).await
    }

    async fn logs_locked(
        &self,
        runtime: RuntimeKind,
        name: &str,
        options: &ServiceLogsOptions,
    ) -> Result<ServiceLogs> {
        self.ensure_service_configuration_loaded(name)?;
        self.ensure_runtime_enabled(runtime)?;
        self.ensure_runtime_service(runtime, name).await?;
        match runtime {
            RuntimeKind::Wasmtime => self.wasmtime.logs(name, options).await,
            RuntimeKind::External => bail!("external TCP services do not have runtime logs"),
            RuntimeKind::Unknown => bail!("unknown service runtime"),
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

    fn ensure_runtime_enabled(&self, runtime: RuntimeKind) -> Result<()> {
        match runtime {
            RuntimeKind::Unknown => bail!("unknown service runtime"),
            RuntimeKind::Wasmtime => {
                if !self.wasmtime_enabled {
                    bail!("wasmtime runtime is disabled in config");
                }
            }
            RuntimeKind::External => {}
        }
        Ok(())
    }

    async fn ensure_runtime_service(&self, runtime: RuntimeKind, name: &str) -> Result<()> {
        self.ensure_service_configuration_loaded(name)?;
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
        if let Some(pending) = self.pending_cleanup.lock().get(name) {
            return Ok(pending.instance.runtime);
        }
        if let Some(failed) = self.service_state.lock().failed_service(name) {
            return Ok(failed.runtime);
        }
        self.service_index
            .lock()
            .get(name)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("service not found: {name}"))
    }

    fn reserved_host_ports(&self) -> BTreeSet<u16> {
        self.reserved_host_ports_except("")
    }

    fn ensure_service_configuration_loaded(&self, name: &str) -> Result<()> {
        if let Some(pending) = self.pending_cleanup.lock().get(name) {
            bail!(
                "service '{}': {}",
                name,
                pending.instance.status.state_label()
            );
        }
        if let Some(failed) = self.service_state.lock().failed_service(name) {
            bail!("service '{}': {}", name, failed.status.state_label());
        }
        Ok(())
    }

    async fn lock_service_operation(&self, name: &str) -> tokio::sync::OwnedMutexGuard<()> {
        let operation = {
            let mut operations = self.service_operations.lock();
            operations.retain(|_, operation| operation.strong_count() > 0);
            match operations.get(name).and_then(Weak::upgrade) {
                Some(operation) => operation,
                None => {
                    let operation = Arc::new(tokio::sync::Mutex::new(()));
                    operations.insert(name.to_string(), Arc::downgrade(&operation));
                    operation
                }
            }
        };
        operation.lock_owned().await
    }

    async fn retry_pending_cleanup(&self, name: &str) -> Result<()> {
        let pending = { self.pending_cleanup.lock().get(name).cloned() };
        if let Some(pending) = pending {
            if let Err(error) = self
                .remove_runtime_only(pending.instance.runtime, name, &pending.local_service_id)
                .await
            {
                let mut failed = pending;
                failed.instance.status = ServiceStatus::unknown().with_detail(format!(
                    "apply persistence error: {}; runtime cleanup error: {error:#}; fix the filesystem/runtime problem and retry apply or remove",
                    failed.apply_error,
                ));
                self.pending_cleanup.lock().insert(name.to_string(), failed);
                return Err(error);
            }
            self.pending_cleanup.lock().remove(name);
        }
        Ok(())
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
            RuntimeKind::Unknown => bail!("unknown service runtime"),
            RuntimeKind::Wasmtime => self.wasmtime.stop(name).await,
            RuntimeKind::External => Ok(()),
        };

        match stop_result {
            Ok(()) => Ok(()),
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
            RuntimeKind::Unknown => bail!("unknown service runtime"),
            RuntimeKind::Wasmtime => {
                self.wasmtime
                    .remove_with_local_service_id(name, local_service_id)
                    .await
            }
            RuntimeKind::External => Ok(()),
        };

        remove_result
    }

    fn external_instance_from_manifest(
        &self,
        manifest: &ServiceManifest,
        running: bool,
    ) -> ServiceInstance {
        ServiceInstance {
            id: format!("external:{}", manifest.name),
            runtime: RuntimeKind::External,
            name: manifest.name.clone(),
            definition_id: manifest.definition_id.clone(),
            source: match &manifest.source {
                ServiceSource::ExistingTcp { host, port } => format!("{host}:{port}"),
                _ => "external".to_string(),
            },
            labels: manifest.labels.clone(),
            ports: manifest.ports.clone(),
            exposed_endpoints: service_expose_endpoint_bindings(manifest),
            status: if running {
                ServiceStatus::running()
            } else {
                ServiceStatus::stopped()
            },
        }
    }
}

fn ensure_definition_id_compatible(
    previous: &ServiceManifest,
    next: &ServiceManifest,
) -> Result<()> {
    ensure_matching_definition_id(
        &next.name,
        previous.definition_id.as_deref(),
        next.definition_id.as_deref(),
    )
}

fn ensure_matching_definition_id(
    name: &str,
    previous: Option<&str>,
    next: Option<&str>,
) -> Result<()> {
    match (previous, next) {
        (Some(previous_id), Some(next_id)) if previous_id != next_id => bail!(
            "service '{}' was created from definition id '{}' and cannot be overwritten with definition id '{}'; use a different service name or remove the existing service first",
            name,
            previous_id,
            next_id
        ),
        (Some(previous_id), None) => bail!(
            "service '{}' was created from definition id '{}' and cannot be overwritten by a manifest without a definition id; use a matching .fungi.md service file or remove the existing service first",
            name,
            previous_id
        ),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod rollback_tests {
    use super::*;
    use std::fs;

    #[tokio::test]
    async fn cleanup_failure_remains_visible_and_allows_retry_or_remove() {
        for retry_apply in [true, false] {
            let temp = tempfile::TempDir::new().unwrap();
            let home = temp.path();
            fs::write(home.join("component.wasm"), b"component").unwrap();
            let manifest = super::super::parse_service_manifest_yaml(
                "fungi: service/v1\nid: demo\nrun:\n  provider: wasmtime\n  source:\n    file: component.wasm\npublish:\n  main:\n    tcp:\n      port: 8082\n", home, home,
            ).unwrap();
            let provider = WasmtimeRuntimeProvider::new(
                home.join("runtime"),
                PathBuf::from("unused"),
                home.to_path_buf(),
                vec![home.to_path_buf()],
            );
            let control = RuntimeControl::with_wasmtime_provider(
                provider.clone(),
                home.join("services"),
                true,
            )
            .unwrap();
            let instance = provider
                .pull_with_local_service_id(&manifest, "svc_pending")
                .await
                .unwrap();
            // Replay the state immediately after registration succeeds and persistence fails.
            control.pending_cleanup.lock().insert(
                "demo".to_string(),
                PendingRuntimeCleanup {
                    local_service_id: "svc_pending".into(),
                    instance,
                    apply_error: "test persistence failure".into(),
                },
            );
            if !retry_apply {
                control.seed_in_memory_service_for_test(manifest.clone());
            }
            let artifacts = home.join("artifacts/services/svc_pending");
            fs::remove_file(artifacts.join("component.wasm")).unwrap();
            fs::remove_dir(&artifacts).unwrap();
            fs::write(&artifacts, b"blocked").unwrap();
            assert!(control.remove_by_name("demo").await.is_err());
            assert!(!provider.has_service("demo"));
            let listed = control.list_services().await.unwrap();
            assert_eq!(listed.len(), 1);
            assert!(
                listed[0]
                    .status
                    .state_label()
                    .contains("runtime cleanup error")
            );
            assert_eq!(
                control.inspect_by_name("demo").await.unwrap().status.phase,
                ServicePhase::Unknown
            );
            assert!(control.start_by_name("demo").await.is_err());

            fs::remove_file(&artifacts).unwrap();
            if retry_apply {
                control.apply(&manifest).await.unwrap();
                assert!(provider.has_service("demo"));
                assert!(control.pending_cleanup.lock().is_empty());
            }
            control.remove_by_name("demo").await.unwrap();
            assert!(control.list_services().await.unwrap().is_empty());
        }
    }

    #[tokio::test]
    async fn service_operation_serialization_does_not_block_other_services() {
        let temp = tempfile::TempDir::new().unwrap();
        let home = temp.path();
        let control = RuntimeControl::new(
            home.join("runtime"),
            PathBuf::from("unused"),
            home.to_path_buf(),
            home.join("services"),
            vec![],
            false,
        )
        .unwrap();
        let manifest = super::super::parse_service_manifest_yaml(
            "fungi: service/v1\nid: demo\npublish:\n  main:\n    tcp:\n      port: 54321\n",
            home,
            home,
        )
        .unwrap();
        let operation = control.lock_service_operation("demo").await;
        let apply = control.apply(&manifest);
        tokio::pin!(apply);
        tokio::select! {
            biased;
            result = &mut apply => panic!("apply bypassed the service operation lock: {result:?}"),
            _ = std::future::ready(()) => {}
        }
        let mut other = manifest.clone();
        other.name = "other".into();
        control.apply(&other).await.unwrap();
        drop(operation);
        apply.await.unwrap();
        assert_eq!(control.list_services().await.unwrap().len(), 2);
    }
}
