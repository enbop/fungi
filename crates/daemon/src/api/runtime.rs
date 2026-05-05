use std::path::{Path, PathBuf};

use anyhow::Result;
use fungi_config::runtime::Runtime as RuntimeConfig;
use libp2p::PeerId;

use crate::runtime::{
    CatalogService, RuntimeKind, ServiceInstance, ServiceLogs, ServiceLogsOptions, ServiceManifest,
    service_expose_endpoint_bindings,
};
use crate::{
    FungiDaemon, LocalRuntimeStatus, ManifestResolutionPolicy, NodeCapabilities,
    ResolvedServiceRecipe, ServiceControlResponse, ServiceRecipeDetail, ServiceRecipeRuntime,
    ServiceRecipeSummary, build_local_node_capabilities, build_local_runtime_status,
};

impl FungiDaemon {
    pub fn get_tcp_tunneling_config(&self) -> fungi_config::tcp_tunneling::TcpTunneling {
        self.config().lock().tcp_tunneling.clone()
    }

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
        let listening_rules = self.get_tcp_listening_rules();

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
        self.runtime_control().pull(&manifest).await
    }

    pub async fn pull_service_from_manifest_yaml(
        &self,
        manifest_yaml: String,
        manifest_base_dir: Option<PathBuf>,
    ) -> Result<ServiceInstance> {
        let fungi_home = self.fungi_home_dir();
        let base_dir = manifest_base_dir.unwrap_or_else(|| fungi_home.clone());
        let policy = self.manifest_resolution_policy();
        self.runtime_control()
            .pull_manifest_yaml(&manifest_yaml, &base_dir, &fungi_home, &policy)
            .await
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

    pub async fn list_exposed_services(&self) -> Result<Vec<CatalogService>> {
        self.runtime_control().list_catalog_services().await
    }

    pub async fn list_peer_services(&self, peer_id: PeerId) -> Result<Vec<CatalogService>> {
        self.service_discovery_control()
            .list_peer_services(peer_id)
            .await
    }

    pub async fn list_catalog_services(&self) -> Result<Vec<CatalogService>> {
        self.list_exposed_services().await
    }

    pub async fn list_peer_catalog(&self, peer_id: PeerId) -> Result<Vec<CatalogService>> {
        self.refresh_peer_services(peer_id).await
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
        let mut resolved =
            crate::recipes::resolve_official_service_recipe(&fungi_dir, recipe_id, service_name, refresh)
                .await?;
        resolved.warnings = self
            .build_service_recipe_runtime_warnings(resolved.detail.summary.runtime, target_peer_id)
            .await;
        Ok(resolved)
    }

    pub fn list_cached_peer_services(&self, peer_id: PeerId) -> Result<Vec<CatalogService>> {
        let peer_id = peer_id.to_string();
        let fungi_dir = self.config_fungi_dir()?;
        let cache = fungi_config::service_cache::ServiceCache::apply_from_dir(&fungi_dir)?;
        let Some(services_json) = cache.get_device_services_json(&peer_id)? else {
            return Ok(Vec::new());
        };
        serde_json::from_str(&services_json)
            .map_err(|error| anyhow::anyhow!("failed to decode cached peer services: {}", error))
    }

    pub async fn refresh_peer_services(&self, peer_id: PeerId) -> Result<Vec<CatalogService>> {
        let services = self.list_peer_services(peer_id).await?;
        self.save_cached_peer_services(peer_id, &services)?;
        Ok(services)
    }

    fn save_cached_peer_services(
        &self,
        peer_id: PeerId,
        services: &[CatalogService],
    ) -> Result<()> {
        let services_json = serde_json::to_string(services)?;
        let fungi_dir = self.config_fungi_dir()?;
        let cache = fungi_config::service_cache::ServiceCache::apply_from_dir(&fungi_dir)?;
        cache.set_device_services_json(peer_id.to_string(), services_json)?;
        Ok(())
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
            Some(peer_id) => self
                .build_remote_recipe_runtime_warnings(runtime, peer_id)
                .await,
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
            ServiceRecipeRuntime::Link => Vec::new(),
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
                )]
            }
        };

        match runtime {
            ServiceRecipeRuntime::Docker if !capabilities.runtimes.docker => {
                vec![format!("Target device {label} does not report Docker runtime support")]
            }
            ServiceRecipeRuntime::Wasmtime if !capabilities.runtimes.wasmtime => {
                vec![format!("Target device {label} does not report Wasmtime runtime support")]
            }
            _ => Vec::new(),
        }
    }

    pub async fn remote_pull_service(
        &self,
        peer_id: PeerId,
        manifest_yaml: String,
    ) -> Result<ServiceControlResponse> {
        self.service_control_protocol_control()
            .pull_peer_service(peer_id, manifest_yaml)
            .await
    }

    pub async fn remote_start_service(
        &self,
        peer_id: PeerId,
        name: String,
    ) -> Result<ServiceControlResponse> {
        self.service_control_protocol_control()
            .start_peer_service(peer_id, name)
            .await
    }

    pub async fn remote_list_services(&self, peer_id: PeerId) -> Result<ServiceControlResponse> {
        self.service_control_protocol_control()
            .list_peer_services(peer_id)
            .await
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
            let _ = self.detach_service_access_by_match(peer_id, &service_key);
        }
        Ok(response)
    }

    pub async fn remote_remove_service(
        &self,
        peer_id: PeerId,
        name: String,
    ) -> Result<ServiceControlResponse> {
        let response = self
            .service_control_protocol_control()
            .remove_peer_service(peer_id, name)
            .await?;
        let service_key = response
            .service
            .as_ref()
            .map(|service| service.name.as_str())
            .unwrap_or_default()
            .to_string();
        if !service_key.is_empty() {
            let _ = self.detach_service_access_by_match(peer_id, &service_key);
        }
        Ok(response)
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
        return vec![format!("{runtime_name} runtime is disabled in local config")];
    }
    if !detected {
        return vec![format!("{runtime_name} runtime does not appear to be available locally")];
    }
    vec![format!("{runtime_name} runtime is configured but not active locally")]
}
