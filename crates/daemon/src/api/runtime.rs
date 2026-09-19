use std::{
    path::{Path, PathBuf},
    time::SystemTime,
};

use anyhow::{Context as _, Result};
use fungi_config::runtime::Runtime as RuntimeConfig;
use libp2p::PeerId;

use crate::runtime::{
    AppliedService, DeviceService, DeviceServiceSnapshot, RuntimeKind, ServiceInstance,
    ServiceLogs, ServiceLogsOptions, ServiceManifest,
};
use crate::service_endpoints::{
    sync_applied_service_endpoint_listeners, sync_service_endpoint_listeners_by_name,
    sync_service_endpoint_listeners_for_manifest,
};
use crate::{
    FungiControl, LocalRuntimeStatus, ManifestResolutionPolicy, NodeCapabilities,
    ResolvedServiceRecipe, ServiceControlResponse, ServiceRecipeDetail, ServiceRecipeRuntime,
    ServiceRecipeSummary, build_local_node_capabilities, build_local_runtime_status,
};

pub struct DeviceServiceSnapshotLookup {
    pub snapshot: DeviceServiceSnapshot,
    pub source: DeviceServiceSnapshotSource,
    pub error: Option<String>,
}

pub enum DeviceServiceSnapshotSource {
    Live,
    Cache,
    Empty,
}

impl DeviceServiceSnapshotSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::Cache => "cache",
            Self::Empty => "empty",
        }
    }
}

impl FungiControl {
    pub fn docker_enabled(&self) -> bool {
        self.settings().runtime().docker_enabled()
    }

    pub fn get_runtime_config(&self) -> RuntimeConfig {
        self.settings().runtime()
    }

    pub fn add_runtime_allowed_host_path(&self, path: PathBuf) -> Result<()> {
        let updated_config = self.settings().add_runtime_allowed_host_path(path)?;
        self.apply_runtime_config_update(updated_config)
    }

    pub fn remove_runtime_allowed_host_path(&self, path: &Path) -> Result<()> {
        let updated_config = self.settings().remove_runtime_allowed_host_path(path)?;
        self.apply_runtime_config_update(updated_config)
    }

    async fn sync_service_endpoint_listeners_by_name(
        &self,
        name: &str,
        enabled: bool,
    ) -> Result<()> {
        sync_service_endpoint_listeners_by_name(
            self.services().runtime(),
            self.services().tcp_tunneling(),
            name,
            enabled,
        )
        .await
    }

    async fn sync_service_endpoint_listeners_for_manifest(
        &self,
        manifest: Option<&ServiceManifest>,
        enabled: bool,
    ) -> Result<()> {
        sync_service_endpoint_listeners_for_manifest(
            self.services().tcp_tunneling(),
            manifest,
            enabled,
        )
        .await
    }

    pub fn supports_runtime(&self, runtime: RuntimeKind) -> bool {
        self.services().runtime().supports(runtime)
    }

    fn fungi_home_dir(&self) -> PathBuf {
        self.settings()
            .fungi_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
    }

    fn manifest_resolution_policy(&self) -> ManifestResolutionPolicy {
        ManifestResolutionPolicy
    }

    fn apply_runtime_config_update(&self, updated_config: fungi_config::FungiConfig) -> Result<()> {
        self.services()
            .apply_runtime_config(&updated_config.runtime)
    }

    pub async fn pull_service(&self, manifest: ServiceManifest) -> Result<ServiceInstance> {
        let mut applied = self.services().runtime().apply(&manifest).await?;
        self.record_applied_service_listener_failure(&mut applied)
            .await;
        if let Some(message) = applied.outcome.failure_summary() {
            anyhow::bail!(message);
        }
        Ok(applied.instance)
    }

    pub async fn pull_service_from_manifest_yaml(
        &self,
        manifest_yaml: String,
        manifest_base_dir: Option<PathBuf>,
    ) -> Result<ServiceInstance> {
        let applied = self
            .apply_service_from_manifest_yaml(manifest_yaml, manifest_base_dir)
            .await?;
        if let Some(message) = applied.outcome.failure_summary() {
            anyhow::bail!(message);
        }
        Ok(applied.instance)
    }

    pub async fn apply_service_from_manifest_yaml(
        &self,
        manifest_yaml: String,
        manifest_base_dir: Option<PathBuf>,
    ) -> Result<AppliedService> {
        let fungi_home = self.fungi_home_dir();
        let base_dir = manifest_base_dir.unwrap_or_else(|| fungi_home.clone());
        let policy = self.manifest_resolution_policy();
        let mut applied = self
            .services()
            .runtime()
            .apply_manifest_yaml(&manifest_yaml, &base_dir, &fungi_home, &policy)
            .await?;
        self.record_applied_service_listener_failure(&mut applied)
            .await;
        Ok(applied)
    }

    async fn record_applied_service_listener_failure(&self, applied: &mut AppliedService) {
        sync_applied_service_endpoint_listeners(
            self.services().runtime(),
            self.services().tcp_tunneling(),
            applied,
        )
        .await;
    }

    pub async fn start_service(&self, runtime: RuntimeKind, name: String) -> Result<()> {
        self.services().runtime().start(runtime, &name).await?;
        self.sync_service_endpoint_listeners_by_name(&name, true)
            .await
    }

    pub async fn start_service_by_name(&self, name: String) -> Result<()> {
        self.devices()
            .local()
            .services()
            .service(name)
            .start()
            .await
    }

    pub async fn stop_service(&self, runtime: RuntimeKind, name: String) -> Result<()> {
        self.services().runtime().stop(runtime, &name).await?;
        self.sync_service_endpoint_listeners_by_name(&name, false)
            .await
    }

    pub async fn stop_service_by_name(&self, name: String) -> Result<()> {
        self.devices().local().services().service(name).stop().await
    }

    pub async fn remove_service(&self, runtime: RuntimeKind, name: String) -> Result<()> {
        let manifest = self.services().runtime().get_service_manifest(&name);
        self.services().runtime().remove(runtime, &name).await?;
        self.sync_service_endpoint_listeners_for_manifest(manifest.as_ref(), false)
            .await
    }

    pub async fn remove_service_by_name(&self, name: String) -> Result<()> {
        self.devices()
            .local()
            .services()
            .service(name)
            .remove()
            .await
    }

    pub async fn inspect_service(
        &self,
        runtime: RuntimeKind,
        name: String,
    ) -> Result<ServiceInstance> {
        self.services().runtime().inspect(runtime, &name).await
    }

    pub async fn inspect_service_by_name(&self, name: String) -> Result<ServiceInstance> {
        self.services().runtime().inspect_by_name(&name).await
    }

    pub async fn get_service_logs(
        &self,
        runtime: RuntimeKind,
        name: String,
        tail: Option<String>,
    ) -> Result<ServiceLogs> {
        self.services()
            .runtime()
            .logs(runtime, &name, &ServiceLogsOptions { tail })
            .await
    }

    pub async fn get_service_logs_by_name(
        &self,
        name: String,
        tail: Option<String>,
    ) -> Result<ServiceLogs> {
        self.services()
            .runtime()
            .logs_by_name(&name, &ServiceLogsOptions { tail })
            .await
    }

    pub async fn list_services(&self) -> Result<Vec<ServiceInstance>> {
        self.services().runtime().list_services().await
    }

    pub async fn list_exposed_services(&self) -> Result<Vec<DeviceService>> {
        self.devices().local().services().published().await
    }

    pub async fn list_peer_services(&self, peer_id: PeerId) -> Result<Vec<DeviceService>> {
        self.devices().peer(peer_id).services().published().await
    }

    pub async fn list_service_recipes(&self, refresh: bool) -> Result<Vec<ServiceRecipeSummary>> {
        let fungi_dir = self.config_fungi_dir()?;
        crate::recipes::list_official_service_recipes(&fungi_dir, refresh).await
    }

    pub async fn get_service_recipe(
        &self,
        recipe_id: &str,
        refresh: bool,
    ) -> Result<ServiceRecipeDetail> {
        let fungi_dir = self.config_fungi_dir()?;
        crate::recipes::get_official_service_recipe(&fungi_dir, recipe_id, refresh).await
    }

    pub async fn resolve_service_recipe(
        &self,
        recipe_id: &str,
        service_name: Option<&str>,
        target_peer_id: Option<PeerId>,
        refresh: bool,
    ) -> Result<ResolvedServiceRecipe> {
        let fungi_dir = self.config_fungi_dir()?;
        let mut resolved = crate::recipes::resolve_official_service_recipe(
            &fungi_dir,
            recipe_id,
            service_name,
            refresh,
        )
        .await?;
        resolved.warnings = self
            .build_service_recipe_runtime_warnings(resolved.detail.summary.runtime, target_peer_id)
            .await;
        Ok(resolved)
    }

    pub async fn get_device_service_snapshot(
        &self,
        device_id: PeerId,
        refresh: bool,
    ) -> Result<DeviceServiceSnapshotLookup> {
        let device_services = self.services().for_device(device_id);
        if refresh {
            match device_services.refresh().await {
                Ok(snapshot) => {
                    return Ok(DeviceServiceSnapshotLookup {
                        snapshot,
                        source: DeviceServiceSnapshotSource::Live,
                        error: None,
                    });
                }
                Err(error) => {
                    if let Some(snapshot) = device_services.snapshot()? {
                        return Ok(DeviceServiceSnapshotLookup {
                            snapshot,
                            source: DeviceServiceSnapshotSource::Cache,
                            error: Some(error.to_string()),
                        });
                    }
                    return Ok(DeviceServiceSnapshotLookup {
                        snapshot: DeviceServiceSnapshot {
                            peer_id: device_id.to_string(),
                            services: Vec::new(),
                            updated_at: SystemTime::now(),
                        },
                        source: DeviceServiceSnapshotSource::Empty,
                        error: Some(error.to_string()),
                    });
                }
            }
        }

        if let Some(snapshot) = device_services.snapshot()? {
            Ok(DeviceServiceSnapshotLookup {
                snapshot,
                source: DeviceServiceSnapshotSource::Cache,
                error: None,
            })
        } else {
            Ok(DeviceServiceSnapshotLookup {
                snapshot: DeviceServiceSnapshot {
                    peer_id: device_id.to_string(),
                    services: Vec::new(),
                    updated_at: SystemTime::now(),
                },
                source: DeviceServiceSnapshotSource::Empty,
                error: None,
            })
        }
    }

    pub async fn forget_device_service(
        &self,
        device_id: PeerId,
        name: &str,
    ) -> Result<ServiceControlResponse> {
        let removed = self
            .devices()
            .peer(device_id)
            .services()
            .service(name)
            .forget_cached_observation()?;
        if !removed {
            anyhow::bail!("cached service not found for device: {name}");
        }
        self.service_access()
            .forget_service(device_id, name)
            .await?;
        Ok(ServiceControlResponse::success_forgotten_locally(
            None,
            name.to_string(),
        ))
    }

    pub fn local_node_capabilities(&self) -> NodeCapabilities {
        let config = self.settings().snapshot();
        build_local_node_capabilities(&config, self.services().runtime())
    }

    pub fn local_runtime_status(&self) -> LocalRuntimeStatus {
        let config = self.settings().snapshot();
        build_local_runtime_status(&config, self.services().runtime())
    }

    pub async fn get_peer_capability_summary(&self, peer_id: PeerId) -> Result<NodeCapabilities> {
        self.devices()
            .node_capabilities()
            .discover_peer_capabilities(peer_id)
            .await
    }

    async fn build_service_recipe_runtime_warnings(
        &self,
        runtime: ServiceRecipeRuntime,
        target_peer_id: Option<PeerId>,
    ) -> Vec<String> {
        match target_peer_id {
            Some(peer_id) => {
                self.build_remote_recipe_runtime_warnings(runtime, peer_id)
                    .await
            }
            None => self.build_local_recipe_runtime_warnings(runtime),
        }
    }

    fn build_local_recipe_runtime_warnings(&self, runtime: ServiceRecipeRuntime) -> Vec<String> {
        let status = self.local_runtime_status();
        match runtime {
            ServiceRecipeRuntime::Docker => runtime_status_warning(
                "Docker",
                status.docker.config_enabled,
                status.docker.detected,
                status.docker.active,
            ),
            ServiceRecipeRuntime::Wasmtime => runtime_status_warning(
                "Wasmtime",
                status.wasmtime.config_enabled,
                status.wasmtime.detected,
                status.wasmtime.active,
            ),
            ServiceRecipeRuntime::Tcp => Vec::new(),
        }
    }

    async fn build_remote_recipe_runtime_warnings(
        &self,
        runtime: ServiceRecipeRuntime,
        peer_id: PeerId,
    ) -> Vec<String> {
        let label = peer_id.to_string();
        let capabilities = match self.get_peer_capability_summary(peer_id).await {
            Ok(capabilities) => capabilities,
            Err(error) => {
                return vec![format!(
                    "Could not verify runtime compatibility for target device {label}: {error}"
                )];
            }
        };

        match runtime {
            ServiceRecipeRuntime::Docker if !capabilities.runtimes.docker => {
                vec![format!(
                    "Target device {label} does not report Docker runtime support"
                )]
            }
            ServiceRecipeRuntime::Wasmtime if !capabilities.runtimes.wasmtime => {
                vec![format!(
                    "Target device {label} does not report Wasmtime runtime support"
                )]
            }
            _ => Vec::new(),
        }
    }

    pub async fn remote_pull_service(
        &self,
        peer_id: PeerId,
        manifest_yaml: String,
    ) -> Result<ServiceControlResponse> {
        let applied = self
            .devices()
            .peer(peer_id)
            .services()
            .apply_manifest_yaml_with_outcome(manifest_yaml, None)
            .await?;
        let service_name = applied.service.name().to_string();
        Ok(match applied.outcome {
            Some(outcome) => ServiceControlResponse::applied(None, service_name, outcome),
            None => ServiceControlResponse::success(None, service_name),
        })
    }

    pub async fn remote_start_service(
        &self,
        peer_id: PeerId,
        name: String,
    ) -> Result<ServiceControlResponse> {
        let service = self.devices().peer(peer_id).services().service(name);
        service.start().await?;
        self.restore_saved_service_access(peer_id, service.name().to_string())
            .await
            .with_context(|| {
                format!(
                    "remote service started, but failed to restore saved local access listeners for {}",
                    service.name()
                )
            })?;
        Ok(ServiceControlResponse::success(
            None,
            service.name().to_string(),
        ))
    }

    pub async fn remote_list_services(&self, peer_id: PeerId) -> Result<ServiceControlResponse> {
        let lookup = self.get_device_service_snapshot(peer_id, true).await?;
        Ok(ServiceControlResponse::success_services(
            None,
            serde_json::to_string(&lookup.snapshot.services)?,
        ))
    }

    pub async fn remote_get_service_logs(
        &self,
        peer_id: PeerId,
        name: String,
        tail: Option<usize>,
    ) -> Result<ServiceLogs> {
        self.devices()
            .peer(peer_id)
            .services()
            .service(name)
            .logs(tail)
            .await
    }

    pub async fn remote_stop_service(
        &self,
        peer_id: PeerId,
        name: String,
    ) -> Result<ServiceControlResponse> {
        let service = self.devices().peer(peer_id).services().service(name);
        service.stop().await?;
        self.service_access()
            .detach(peer_id, service.name())
            .with_context(|| {
                format!(
                    "remote service stopped, but failed to disconnect local access listeners for {}",
                    service.name()
                )
            })?;
        Ok(ServiceControlResponse::success(
            None,
            service.name().to_string(),
        ))
    }

    pub async fn remote_remove_service(
        &self,
        peer_id: PeerId,
        name: String,
    ) -> Result<ServiceControlResponse> {
        let service = self.devices().peer(peer_id).services().service(name);
        service.remove().await?;
        self.service_access()
            .forget_service(peer_id, service.name())
            .await
            .with_context(|| {
                format!(
                    "remote service removed, but failed to forget local access records for {}",
                    service.name()
                )
            })?;
        Ok(ServiceControlResponse::success(
            None,
            service.name().to_string(),
        ))
    }
}

fn runtime_status_warning(
    runtime_name: &str,
    config_enabled: bool,
    detected: bool,
    active: bool,
) -> Vec<String> {
    if active {
        return Vec::new();
    }
    if !config_enabled {
        return vec![format!(
            "{runtime_name} runtime is disabled in local config"
        )];
    }
    if !detected {
        return vec![format!(
            "{runtime_name} runtime does not appear to be available locally"
        )];
    }
    vec![format!(
        "{runtime_name} runtime is configured but not active locally"
    )]
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::services::merge_device_service_snapshot;
    use crate::test_support::TestDaemon;
    use crate::{
        DeviceServiceEndpoint, ServiceExposeUsage, ServiceExposeUsageKind, ServicePhase,
        ServicePort, ServicePortAllocation, ServicePortProtocol, ServiceStatus,
    };

    use super::*;

    #[test]
    fn merges_managed_status_with_published_connectable_endpoint() {
        let peer_id = PeerId::random();
        let snapshot = merge_device_service_snapshot(
            peer_id,
            vec![service_instance("web", ServiceStatus::running())],
            vec![published_service("web", ServiceStatus::running())],
        );

        assert_eq!(snapshot.peer_id, peer_id.to_string());
        assert_eq!(snapshot.services.len(), 1);
        let service = &snapshot.services[0];
        assert_eq!(service.name, "web");
        assert_eq!(service.status.phase, ServicePhase::Running);
        assert_eq!(service.endpoints.len(), 1);
        assert_eq!(
            service.metadata.usage.as_ref().unwrap().kind,
            ServiceExposeUsageKind::Web
        );
        let snapshot_json = serde_json::to_value(&snapshot).unwrap();
        let service_json = &snapshot_json["services"][0];
        assert!(service_json.get("ports").is_none());
        assert!(service_json["endpoints"][0].get("service_port").is_none());
        assert!(service_json["endpoints"][0].get("host_port").is_none());
    }

    #[test]
    fn keeps_stopped_managed_service_without_connectable_endpoint() {
        let snapshot = merge_device_service_snapshot(
            PeerId::random(),
            vec![service_instance("web", ServiceStatus::stopped())],
            Vec::new(),
        );

        assert_eq!(snapshot.services.len(), 1);
        let service = &snapshot.services[0];
        assert_eq!(service.name, "web");
        assert_eq!(service.status.phase, ServicePhase::Stopped);
        assert!(service.endpoints.is_empty());
    }

    #[test]
    fn keeps_discovery_only_service_from_published_snapshot() {
        let snapshot = merge_device_service_snapshot(
            PeerId::random(),
            Vec::new(),
            vec![published_service("web", ServiceStatus::running())],
        );

        assert_eq!(snapshot.services.len(), 1);
        let service = &snapshot.services[0];
        assert_eq!(service.name, "web");
        assert_eq!(service.status.phase, ServicePhase::Running);
        assert_eq!(service.endpoints.len(), 1);
    }

    #[tokio::test]
    async fn removes_cached_device_service_snapshot_for_device() {
        let daemon = TestDaemon::spawn().await.unwrap();
        let peer_id = PeerId::random();
        let fungi_dir = daemon.daemon().config_fungi_dir().unwrap();
        let cache =
            fungi_config::service_cache::DeviceServiceSnapshotCache::apply_from_dir(&fungi_dir)
                .unwrap();

        let snapshot = DeviceServiceSnapshot {
            peer_id: peer_id.to_string(),
            services: vec![DeviceService {
                name: "svc-a".to_string(),
                runtime: RuntimeKind::External,
                metadata: Default::default(),
                endpoints: Vec::new(),
                status: ServiceStatus::running(),
            }],
            updated_at: SystemTime::now(),
        };
        cache
            .set_device_snapshot_json(
                peer_id.to_string(),
                serde_json::to_string(&snapshot).unwrap(),
            )
            .unwrap();

        assert!(
            cache
                .get_device_snapshot_json(&peer_id.to_string())
                .unwrap()
                .is_some()
        );

        assert!(daemon.daemon().services().remove_snapshot(peer_id).unwrap());
        assert!(
            cache
                .get_device_snapshot_json(&peer_id.to_string())
                .unwrap()
                .is_none()
        );
    }

    fn service_instance(name: &str, status: ServiceStatus) -> ServiceInstance {
        ServiceInstance {
            id: format!("external:{name}"),
            runtime: RuntimeKind::External,
            name: name.to_string(),
            definition_id: None,
            source: "127.0.0.1".to_string(),
            labels: BTreeMap::new(),
            ports: vec![service_port("web")],
            exposed_endpoints: Vec::new(),
            status,
        }
    }

    fn published_service(name: &str, status: ServiceStatus) -> DeviceService {
        DeviceService {
            name: name.to_string(),
            runtime: RuntimeKind::External,
            metadata: crate::DeviceServiceMetadata {
                usage: Some(ServiceExposeUsage {
                    kind: ServiceExposeUsageKind::Web,
                    path: Some("/".to_string()),
                }),
                icon_url: Some("https://example.test/icon.svg".to_string()),
            },
            endpoints: vec![DeviceServiceEndpoint {
                name: "web".to_string(),
                protocol: format!("/fungi/service/{name}/web/0.2.0"),
            }],
            status,
        }
    }

    fn service_port(name: &str) -> ServicePort {
        ServicePort {
            name: Some(name.to_string()),
            host_port: 18080,
            host_port_allocation: ServicePortAllocation::Fixed,
            service_port: 8080,
            protocol: ServicePortProtocol::Tcp,
        }
    }
}
