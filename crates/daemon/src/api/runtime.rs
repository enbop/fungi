use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::SystemTime,
};

use anyhow::{Context as _, Result};
use fungi_config::runtime::Runtime as RuntimeConfig;
use libp2p::PeerId;

use crate::runtime::{
    DeviceService, DeviceServiceSnapshot, RuntimeKind, ServiceInstance, ServiceLogs,
    ServiceLogsOptions, ServiceManifest, service_expose_endpoint_bindings,
};
use crate::service_state::DesiredServiceState;
use crate::{
    FungiDaemon, LocalRuntimeStatus, ManifestResolutionPolicy, NodeCapabilities,
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

impl FungiDaemon {
    pub fn docker_enabled(&self) -> bool {
        self.config().lock().runtime.docker_enabled()
    }

    pub fn get_runtime_config(&self) -> RuntimeConfig {
        self.config().lock().get_runtime_config()
    }

    pub fn add_runtime_allowed_host_path(&self, path: PathBuf) -> Result<()> {
        let current_config = self.config().lock().clone();
        let updated_config = current_config.add_runtime_allowed_host_path(path)?;
        self.apply_runtime_config_update(updated_config)
    }

    pub fn remove_runtime_allowed_host_path(&self, path: &Path) -> Result<()> {
        let current_config = self.config().lock().clone();
        let updated_config = current_config.remove_runtime_allowed_host_path(path)?;
        self.apply_runtime_config_update(updated_config)
    }

    async fn sync_service_endpoint_listeners_by_name(
        &self,
        name: &str,
        enabled: bool,
    ) -> Result<()> {
        let manifest = self.runtime_control().get_service_manifest(name);
        self.sync_service_endpoint_listeners_for_manifest(manifest.as_ref(), enabled)
            .await
    }

    async fn sync_service_endpoint_listeners_for_manifest(
        &self,
        manifest: Option<&ServiceManifest>,
        enabled: bool,
    ) -> Result<()> {
        let Some(manifest) = manifest else {
            return Ok(());
        };

        let endpoints = service_expose_endpoint_bindings(manifest);
        let listening_rules = self.get_service_endpoint_listening_rules();

        for endpoint in endpoints {
            let existing_rule_id = listening_rules
                .iter()
                .find(|(_, rule)| {
                    rule.port == endpoint.host_port
                        && rule.protocol.as_deref() == Some(endpoint.protocol.as_str())
                })
                .map(|(rule_id, _)| rule_id.clone());

            if enabled {
                if existing_rule_id.is_none() {
                    self.tcp_tunneling_control()
                        .add_listening_rule(fungi_config::tcp_tunneling::ListeningRule {
                            host: "127.0.0.1".to_string(),
                            port: endpoint.host_port,
                            protocol: Some(endpoint.protocol),
                        })
                        .await?;
                }
            } else if let Some(rule_id) = existing_rule_id {
                self.tcp_tunneling_control()
                    .remove_listening_rule(&rule_id)?;
            }
        }

        Ok(())
    }

    pub fn supports_runtime(&self, runtime: RuntimeKind) -> bool {
        self.runtime_control().supports(runtime)
    }

    fn fungi_home_dir(&self) -> PathBuf {
        self.config()
            .lock()
            .config_file_path()
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .to_path_buf()
    }

    fn manifest_resolution_policy(&self) -> ManifestResolutionPolicy {
        ManifestResolutionPolicy::default()
    }

    fn apply_runtime_config_update(&self, updated_config: fungi_config::FungiConfig) -> Result<()> {
        if let Some(docker_control) = self.docker_control() {
            docker_control.update_runtime_config(&updated_config.runtime)?;
        }
        self.runtime_control()
            .update_allowed_host_paths(updated_config.runtime.allowed_host_paths.clone());
        *self.config().lock() = updated_config;
        Ok(())
    }

    pub async fn pull_service(&self, manifest: ServiceManifest) -> Result<ServiceInstance> {
        let applied = self.runtime_control().apply(&manifest).await?;
        if applied.desired_state == DesiredServiceState::Running {
            self.sync_service_endpoint_listeners_for_manifest(
                applied.previous_manifest.as_ref(),
                false,
            )
            .await?;
            self.sync_service_endpoint_listeners_by_name(&applied.instance.name, true)
                .await?;
        }
        Ok(applied.instance)
    }

    pub async fn pull_service_from_manifest_yaml(
        &self,
        manifest_yaml: String,
        manifest_base_dir: Option<PathBuf>,
    ) -> Result<ServiceInstance> {
        let fungi_home = self.fungi_home_dir();
        let base_dir = manifest_base_dir.unwrap_or_else(|| fungi_home.clone());
        let policy = self.manifest_resolution_policy();
        let applied = self
            .runtime_control()
            .apply_manifest_yaml(&manifest_yaml, &base_dir, &fungi_home, &policy)
            .await?;
        if applied.desired_state == DesiredServiceState::Running {
            self.sync_service_endpoint_listeners_for_manifest(
                applied.previous_manifest.as_ref(),
                false,
            )
            .await?;
            self.sync_service_endpoint_listeners_by_name(&applied.instance.name, true)
                .await?;
        }
        Ok(applied.instance)
    }

    pub async fn start_service(&self, runtime: RuntimeKind, name: String) -> Result<()> {
        self.runtime_control().start(runtime, &name).await?;
        self.sync_service_endpoint_listeners_by_name(&name, true)
            .await
    }

    pub async fn start_service_by_name(&self, name: String) -> Result<()> {
        self.runtime_control().start_by_name(&name).await?;
        self.sync_service_endpoint_listeners_by_name(&name, true)
            .await
    }

    pub async fn stop_service(&self, runtime: RuntimeKind, name: String) -> Result<()> {
        self.runtime_control().stop(runtime, &name).await?;
        self.sync_service_endpoint_listeners_by_name(&name, false)
            .await
    }

    pub async fn stop_service_by_name(&self, name: String) -> Result<()> {
        self.runtime_control().stop_by_name(&name).await?;
        self.sync_service_endpoint_listeners_by_name(&name, false)
            .await
    }

    pub async fn remove_service(&self, runtime: RuntimeKind, name: String) -> Result<()> {
        let manifest = self.runtime_control().get_service_manifest(&name);
        self.runtime_control().remove(runtime, &name).await?;
        self.sync_service_endpoint_listeners_for_manifest(manifest.as_ref(), false)
            .await
    }

    pub async fn remove_service_by_name(&self, name: String) -> Result<()> {
        let manifest = self.runtime_control().get_service_manifest(&name);
        self.runtime_control().remove_by_name(&name).await?;
        self.sync_service_endpoint_listeners_for_manifest(manifest.as_ref(), false)
            .await
    }

    pub async fn inspect_service(
        &self,
        runtime: RuntimeKind,
        name: String,
    ) -> Result<ServiceInstance> {
        self.runtime_control().inspect(runtime, &name).await
    }

    pub async fn inspect_service_by_name(&self, name: String) -> Result<ServiceInstance> {
        self.runtime_control().inspect_by_name(&name).await
    }

    pub async fn get_service_logs(
        &self,
        runtime: RuntimeKind,
        name: String,
        tail: Option<String>,
    ) -> Result<ServiceLogs> {
        self.runtime_control()
            .logs(runtime, &name, &ServiceLogsOptions { tail })
            .await
    }

    pub async fn get_service_logs_by_name(
        &self,
        name: String,
        tail: Option<String>,
    ) -> Result<ServiceLogs> {
        self.runtime_control()
            .logs_by_name(&name, &ServiceLogsOptions { tail })
            .await
    }

    pub async fn list_services(&self) -> Result<Vec<ServiceInstance>> {
        self.runtime_control().list_services().await
    }

    pub async fn list_exposed_services(&self) -> Result<Vec<DeviceService>> {
        self.runtime_control()
            .list_published_device_services()
            .await
    }

    pub async fn list_peer_services(&self, peer_id: PeerId) -> Result<Vec<DeviceService>> {
        self.service_discovery_control()
            .list_peer_services(peer_id)
            .await
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

    pub fn load_device_service_snapshot(
        &self,
        device_id: PeerId,
    ) -> Result<Option<DeviceServiceSnapshot>> {
        let fungi_dir = self.config_fungi_dir()?;
        let cache =
            fungi_config::service_cache::DeviceServiceSnapshotCache::apply_from_dir(&fungi_dir)?;
        let Some(snapshot_json) = cache.get_device_snapshot_json(&device_id.to_string())? else {
            return Ok(None);
        };
        serde_json::from_str(&snapshot_json)
            .map(Some)
            .map_err(|error| {
                anyhow::anyhow!("failed to decode cached device service snapshot: {}", error)
            })
    }

    pub async fn refresh_device_service_snapshot(
        &self,
        device_id: PeerId,
    ) -> Result<DeviceServiceSnapshot> {
        let managed_response = self
            .service_control_protocol_control()
            .list_peer_services(device_id)
            .await;
        let published = self.list_peer_services(device_id).await;

        let managed_response = managed_response.with_context(|| {
            format!("failed to refresh managed services for device {device_id}")
        })?;
        let published = published.with_context(|| {
            format!("failed to refresh published services for device {device_id}")
        })?;

        let managed = managed_response
            .services_json
            .as_deref()
            .map(|services_json| serde_json::from_str::<Vec<ServiceInstance>>(services_json))
            .transpose()
            .map_err(|error| {
                anyhow::anyhow!(
                    "failed to decode managed services from device {device_id}: {error}"
                )
            })?
            .unwrap_or_default();

        let snapshot = merge_device_service_snapshot(device_id, managed, published);
        self.save_device_service_snapshot(&snapshot)?;
        Ok(snapshot)
    }

    pub fn save_device_service_snapshot(&self, snapshot: &DeviceServiceSnapshot) -> Result<()> {
        let snapshot_json = serde_json::to_string(snapshot)?;
        let fungi_dir = self.config_fungi_dir()?;
        let cache =
            fungi_config::service_cache::DeviceServiceSnapshotCache::apply_from_dir(&fungi_dir)?;
        cache.set_device_snapshot_json(snapshot.peer_id.clone(), snapshot_json)?;
        Ok(())
    }

    pub fn remove_device_service_snapshot(&self, device_id: PeerId) -> Result<bool> {
        let fungi_dir = self.config_fungi_dir()?;
        let cache =
            fungi_config::service_cache::DeviceServiceSnapshotCache::apply_from_dir(&fungi_dir)?;
        cache.remove_device_snapshot(&device_id.to_string())
    }

    pub async fn get_device_service_snapshot(
        &self,
        device_id: PeerId,
        refresh: bool,
    ) -> Result<DeviceServiceSnapshotLookup> {
        if refresh {
            match self.refresh_device_service_snapshot(device_id).await {
                Ok(snapshot) => {
                    return Ok(DeviceServiceSnapshotLookup {
                        snapshot,
                        source: DeviceServiceSnapshotSource::Live,
                        error: None,
                    });
                }
                Err(error) => {
                    if let Some(snapshot) = self.load_device_service_snapshot(device_id)? {
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

        if let Some(snapshot) = self.load_device_service_snapshot(device_id)? {
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

    async fn refresh_or_keep_device_service_snapshot(&self, device_id: PeerId) {
        if let Err(refresh_error) = self.refresh_device_service_snapshot(device_id).await {
            log::warn!(
                "Failed to refresh device service snapshot for device {device_id} after remote mutation: {refresh_error}"
            );
        }
    }

    fn remove_cached_device_service(&self, device_id: PeerId, name: &str) -> Result<bool> {
        let Some(mut snapshot) = self.load_device_service_snapshot(device_id)? else {
            return Ok(false);
        };
        let before = snapshot.services.len();
        snapshot.services.retain(|service| service.name != name);
        if snapshot.services.len() == before {
            return Ok(false);
        }

        self.save_device_service_snapshot(&snapshot)?;
        Ok(true)
    }

    pub async fn forget_device_service(
        &self,
        device_id: PeerId,
        name: &str,
    ) -> Result<ServiceControlResponse> {
        if !self.remove_cached_device_service(device_id, name)? {
            anyhow::bail!("cached service not found for device: {name}");
        }

        self.forget_service_access(device_id, name.to_string())
            .await
            .with_context(|| format!("failed to forget local access records for service {name}"))?;
        Ok(ServiceControlResponse::success_forgotten_locally(
            None,
            name.to_string(),
        ))
    }

    pub fn local_node_capabilities(&self) -> NodeCapabilities {
        let config = self.config().lock().clone();
        build_local_node_capabilities(&config, self.runtime_control())
    }

    pub fn local_runtime_status(&self) -> LocalRuntimeStatus {
        let config = self.config().lock().clone();
        build_local_runtime_status(&config, self.runtime_control())
    }

    pub async fn get_peer_capability_summary(&self, peer_id: PeerId) -> Result<NodeCapabilities> {
        self.node_capabilities_control()
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
        let response = self
            .service_control_protocol_control()
            .pull_peer_service(peer_id, manifest_yaml)
            .await?;
        self.refresh_or_keep_device_service_snapshot(peer_id).await;
        Ok(response)
    }

    pub async fn remote_start_service(
        &self,
        peer_id: PeerId,
        name: String,
    ) -> Result<ServiceControlResponse> {
        let response = self
            .service_control_protocol_control()
            .start_peer_service(peer_id, name.clone())
            .await?;
        let service_key = response
            .service
            .as_ref()
            .map(|service| service.name.as_str())
            .unwrap_or(name.as_str())
            .to_string();
        self.refresh_or_keep_device_service_snapshot(peer_id).await;
        self.restore_saved_service_access(peer_id, service_key.clone())
            .await
            .with_context(|| {
                format!(
                    "remote service started, but failed to restore saved local access listeners for {service_key}"
                )
            })?;
        Ok(response)
    }

    pub async fn remote_list_services(&self, peer_id: PeerId) -> Result<ServiceControlResponse> {
        let lookup = self.get_device_service_snapshot(peer_id, true).await?;
        Ok(ServiceControlResponse::success_services(
            None,
            serde_json::to_string(&lookup.snapshot.services)?,
        ))
    }

    pub async fn remote_stop_service(
        &self,
        peer_id: PeerId,
        name: String,
    ) -> Result<ServiceControlResponse> {
        let response = self
            .service_control_protocol_control()
            .stop_peer_service(peer_id, name)
            .await?;
        let service_key = response
            .service
            .as_ref()
            .map(|service| service.name.as_str())
            .unwrap_or_default()
            .to_string();
        if !service_key.is_empty() {
            self.detach_service_access_by_match(peer_id, &service_key)
                .with_context(|| {
                    format!(
                        "remote service stopped, but failed to disconnect local access listeners for {service_key}"
                    )
                })?;
        }
        self.refresh_or_keep_device_service_snapshot(peer_id).await;
        Ok(response)
    }

    pub async fn remote_remove_service(
        &self,
        peer_id: PeerId,
        name: String,
    ) -> Result<ServiceControlResponse> {
        let response = self
            .service_control_protocol_control()
            .remove_peer_service(peer_id, name.clone())
            .await?;
        let service_key = response
            .service
            .as_ref()
            .map(|service| service.name.as_str())
            .unwrap_or_default()
            .to_string();
        if !service_key.is_empty() {
            self.forget_service_access(peer_id, service_key.clone())
                .await
                .with_context(|| {
                    format!(
                        "remote service removed, but failed to forget local access records for {service_key}"
                    )
                })?;
        }
        self.refresh_or_keep_device_service_snapshot(peer_id).await;
        Ok(response)
    }
}

fn merge_device_service_snapshot(
    device_id: PeerId,
    managed: Vec<ServiceInstance>,
    published: Vec<DeviceService>,
) -> DeviceServiceSnapshot {
    let mut services_by_name = BTreeMap::<String, DeviceService>::new();

    for service in managed {
        services_by_name.insert(service.name.clone(), device_service_from_instance(service));
    }

    for service in published {
        services_by_name
            .entry(service.name.clone())
            .and_modify(|existing| {
                existing.metadata = service.metadata.clone();
                existing.endpoints = service.endpoints.clone();
            })
            .or_insert(service);
    }

    DeviceServiceSnapshot {
        peer_id: device_id.to_string(),
        services: services_by_name.into_values().collect(),
        updated_at: SystemTime::now(),
    }
}

fn device_service_from_instance(instance: ServiceInstance) -> DeviceService {
    DeviceService {
        name: instance.name,
        runtime: instance.runtime,
        metadata: Default::default(),
        endpoints: Vec::new(),
        status: instance.status,
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

        assert!(
            daemon
                .daemon()
                .remove_device_service_snapshot(peer_id)
                .unwrap()
        );
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
