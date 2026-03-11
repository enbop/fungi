use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs::{self, OpenOptions},
    io::Read,
    net::{TcpListener as StdTcpListener, UdpSocket as StdUdpSocket},
    path::{Component, Path, PathBuf},
    process::Stdio,
    sync::Arc,
};

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use fungi_config::runtime::AllowedPortRange;
use fungi_docker_agent::{ContainerSpec, LogsOptions, PortProtocol};
use fungi_util::protocols::service_port_protocol;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tokio::process::{Child, Command};

use crate::{
    controls::DockerControl,
    service_state::{DesiredServiceState, PersistedService, ServiceStateStore},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeKind {
    Docker,
    Wasmtime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceManifest {
    pub name: String,
    pub runtime: RuntimeKind,
    pub source: ServiceSource,
    pub expose: Option<ServiceExpose>,
    pub env: BTreeMap<String, String>,
    pub mounts: Vec<ServiceMount>,
    pub ports: Vec<ServicePort>,
    pub command: Vec<String>,
    pub entrypoint: Vec<String>,
    pub working_dir: Option<String>,
    pub labels: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServiceSource {
    Docker { image: String },
    WasmtimeFile { component: PathBuf },
    WasmtimeUrl { url: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceExpose {
    pub service_id: String,
    pub display_name: String,
    pub transport: ServiceExposeTransport,
    pub usage: Option<ServiceExposeUsage>,
    pub icon_url: Option<String>,
    pub catalog_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceExposeTransport {
    pub kind: ServiceExposeTransportKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceExposeTransportKind {
    Tcp,
    Raw,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceExposeUsage {
    pub kind: ServiceExposeUsageKind,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceExposeUsageKind {
    Web,
    Ssh,
    Raw,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceMount {
    pub host_path: PathBuf,
    pub runtime_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServicePort {
    pub name: Option<String>,
    pub host_port: u16,
    pub service_port: u16,
    pub protocol: ServicePortProtocol,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServicePortProtocol {
    Tcp,
    Udp,
}

#[derive(Debug, Clone, Default)]
pub struct ManifestResolutionPolicy {
    pub allowed_tcp_ports: Vec<u16>,
    pub allowed_tcp_port_ranges: Vec<AllowedPortRange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInstance {
    pub runtime: RuntimeKind,
    pub handle: String,
    pub name: String,
    pub source: String,
    pub labels: BTreeMap<String, String>,
    #[serde(default)]
    pub ports: Vec<ServicePort>,
    #[serde(default)]
    pub exposed_endpoints: Vec<ServiceExposeEndpointBinding>,
    pub status: ServiceStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceStatus {
    pub state: String,
    pub running: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredService {
    pub service_name: String,
    pub service_id: String,
    pub display_name: String,
    pub runtime: RuntimeKind,
    pub transport: ServiceExposeTransport,
    pub usage: Option<ServiceExposeUsage>,
    pub icon_url: Option<String>,
    pub catalog_id: Option<String>,
    #[serde(default)]
    pub endpoints: Vec<DiscoveredServiceEndpoint>,
    pub status: ServiceStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredServiceEndpoint {
    pub name: String,
    pub protocol: String,
    pub service_port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceExposeEndpointBinding {
    pub name: String,
    pub protocol: String,
    pub host_port: u16,
    pub service_port: u16,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServiceLogsOptions {
    pub tail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceLogs {
    pub raw: Vec<u8>,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceManifestDocument {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    pub metadata: ServiceManifestMetadata,
    pub spec: ServiceManifestSpec,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServiceManifestMetadata {
    pub name: String,
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceManifestSpec {
    pub runtime: RuntimeKind,
    pub source: ServiceManifestSource,
    pub expose: Option<ServiceManifestExpose>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub mounts: Vec<ServiceManifestMount>,
    #[serde(default)]
    pub ports: Vec<ServiceManifestPort>,
    #[serde(default)]
    pub command: Vec<String>,
    #[serde(default)]
    pub entrypoint: Vec<String>,
    #[serde(rename = "workingDir")]
    pub working_dir: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServiceManifestSource {
    pub image: Option<String>,
    pub file: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceManifestMount {
    #[serde(rename = "hostPath")]
    pub host_path: String,
    #[serde(rename = "runtimePath")]
    pub runtime_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceManifestPort {
    #[serde(rename = "hostPort")]
    pub host_port: ServiceManifestHostPort,
    #[serde(rename = "servicePort")]
    pub service_port: u16,
    #[serde(default)]
    pub name: Option<String>,
    pub protocol: ServicePortProtocol,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ServiceManifestHostPort {
    Fixed(u16),
    Keyword(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceManifestExpose {
    #[serde(default)]
    pub enabled: bool,
    #[serde(rename = "serviceId")]
    pub service_id: Option<String>,
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
    pub transport: Option<ServiceManifestExposeTransport>,
    pub usage: Option<ServiceManifestExposeUsage>,
    #[serde(rename = "iconUrl")]
    pub icon_url: Option<String>,
    #[serde(rename = "catalogId")]
    pub catalog_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceManifestExposeTransport {
    pub kind: ServiceExposeTransportKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceManifestExposeUsage {
    pub kind: ServiceExposeUsageKind,
    pub path: Option<String>,
}

pub fn load_service_manifest_yaml_file(path: &Path, fungi_home: &Path) -> Result<ServiceManifest> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read service manifest: {}", path.display()))?;
    let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
    parse_service_manifest_yaml(&content, base_dir, fungi_home)
}

pub fn parse_service_manifest_yaml(
    content: &str,
    base_dir: &Path,
    fungi_home: &Path,
) -> Result<ServiceManifest> {
    parse_service_manifest_yaml_with_policy(
        content,
        base_dir,
        fungi_home,
        &ManifestResolutionPolicy::default(),
        &BTreeSet::new(),
    )
}

pub fn parse_service_manifest_yaml_with_policy(
    content: &str,
    base_dir: &Path,
    fungi_home: &Path,
    policy: &ManifestResolutionPolicy,
    used_host_ports: &BTreeSet<u16>,
) -> Result<ServiceManifest> {
    let document: ServiceManifestDocument =
        serde_yaml::from_str(content).context("Failed to parse service manifest YAML")?;
    document.into_service_manifest_for_node(base_dir, fungi_home, policy, used_host_ports)
}

impl ServiceManifestDocument {
    pub fn into_service_manifest(
        self,
        base_dir: &Path,
        fungi_home: &Path,
    ) -> Result<ServiceManifest> {
        self.into_service_manifest_for_node(
            base_dir,
            fungi_home,
            &ManifestResolutionPolicy::default(),
            &BTreeSet::new(),
        )
    }

    pub fn into_service_manifest_for_node(
        self,
        base_dir: &Path,
        fungi_home: &Path,
        policy: &ManifestResolutionPolicy,
        used_host_ports: &BTreeSet<u16>,
    ) -> Result<ServiceManifest> {
        if self.kind != "ServiceManifest" {
            bail!("Unsupported manifest kind: {}", self.kind);
        }

        let ServiceManifestDocument {
            api_version: _,
            kind: _,
            metadata,
            spec,
        } = self;
        let service_name = metadata.name;
        let metadata_labels = metadata.labels;
        let app_home = fungi_home.join("services").join(&service_name);
        let mut reserved_host_ports = used_host_ports.clone();

        let runtime = spec.runtime;
        let source = match runtime {
            RuntimeKind::Docker => {
                let Some(image) = spec.source.image else {
                    bail!("docker service manifest requires spec.source.image");
                };
                ServiceSource::Docker { image }
            }
            RuntimeKind::Wasmtime => match (spec.source.file, spec.source.url) {
                (Some(file), None) => ServiceSource::WasmtimeFile {
                    component: resolve_manifest_path(&file, base_dir, fungi_home, &app_home),
                },
                (None, Some(url)) => ServiceSource::WasmtimeUrl { url },
                (Some(_), Some(_)) => {
                    bail!(
                        "wasmtime service manifest accepts only one of spec.source.file or spec.source.url"
                    )
                }
                (None, None) => {
                    bail!("wasmtime service manifest requires spec.source.file or spec.source.url")
                }
            },
        };

        let ports = spec
            .ports
            .into_iter()
            .map(|port| {
                Ok(ServicePort {
                    name: normalize_optional(port.name),
                    host_port: resolve_manifest_host_port(
                        port.host_port,
                        port.protocol,
                        policy,
                        &mut reserved_host_ports,
                    )?,
                    service_port: port.service_port,
                    protocol: port.protocol,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        let expose = parse_manifest_expose(spec.expose, &service_name)?;
        validate_manifest_exposed_ports(expose.as_ref(), &ports)?;

        Ok(ServiceManifest {
            name: service_name.clone(),
            runtime,
            source,
            expose,
            env: spec.env,
            mounts: spec
                .mounts
                .into_iter()
                .map(|mount| ServiceMount {
                    host_path: resolve_manifest_path(
                        &mount.host_path,
                        base_dir,
                        fungi_home,
                        &app_home,
                    ),
                    runtime_path: mount.runtime_path,
                })
                .collect(),
            ports,
            command: spec.command,
            entrypoint: spec.entrypoint,
            working_dir: spec
                .working_dir
                .map(|value| resolve_manifest_path_string(&value, base_dir, fungi_home, &app_home)),
            labels: metadata_labels,
        })
    }
}

fn parse_manifest_expose(
    expose: Option<ServiceManifestExpose>,
    manifest_name: &str,
) -> Result<Option<ServiceExpose>> {
    let Some(expose) = expose else {
        return Ok(None);
    };

    if !expose.enabled {
        return Ok(None);
    }

    let transport = expose.transport.ok_or_else(|| {
        anyhow::anyhow!("spec.expose.transport is required when expose.enabled=true")
    })?;
    let service_id = normalize_non_empty(
        expose.service_id.as_deref().unwrap_or(manifest_name),
        "spec.expose.serviceId",
    )?;
    let display_name = normalize_non_empty(
        expose.display_name.as_deref().unwrap_or(manifest_name),
        "spec.expose.displayName",
    )?;
    let icon_url = normalize_optional(expose.icon_url);
    let catalog_id = normalize_optional(expose.catalog_id);
    let usage = expose.usage.map(|usage| ServiceExposeUsage {
        kind: usage.kind,
        path: normalize_optional(usage.path),
    });

    Ok(Some(ServiceExpose {
        service_id,
        display_name,
        transport: ServiceExposeTransport {
            kind: transport.kind,
        },
        usage,
        icon_url,
        catalog_id,
    }))
}

fn normalize_non_empty(value: &str, field_name: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("{field_name} must not be empty");
    }
    Ok(trimmed.to_string())
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn validate_manifest_exposed_ports(
    expose: Option<&ServiceExpose>,
    ports: &[ServicePort],
) -> Result<()> {
    let Some(expose) = expose else {
        return Ok(());
    };

    if expose.transport.kind != ServiceExposeTransportKind::Tcp {
        return Ok(());
    }

    if ports.iter().any(|port| {
        port.protocol == ServicePortProtocol::Tcp
            && port.name.as_ref().is_some_and(|name| !name.is_empty())
    }) {
        return Ok(());
    }

    bail!(
        "spec.expose.enabled=true with tcp transport requires at least one named TCP port in spec.ports[].name"
    )
}

pub fn service_expose_endpoint_bindings(
    manifest: &ServiceManifest,
) -> Vec<ServiceExposeEndpointBinding> {
    let Some(expose) = &manifest.expose else {
        return Vec::new();
    };

    if expose.transport.kind != ServiceExposeTransportKind::Tcp {
        return Vec::new();
    }

    let mut endpoints = manifest
        .ports
        .iter()
        .filter(|port| port.protocol == ServicePortProtocol::Tcp)
        .filter_map(|port| {
            let name = port.name.as_ref()?.trim();
            if name.is_empty() {
                return None;
            }

            Some(ServiceExposeEndpointBinding {
                name: name.to_string(),
                protocol: service_port_protocol(&expose.service_id, name),
                host_port: port.host_port,
                service_port: port.service_port,
            })
        })
        .collect::<Vec<_>>();

    endpoints.sort_by(|left, right| left.name.cmp(&right.name));
    endpoints
}

fn resolve_manifest_host_port(
    host_port: ServiceManifestHostPort,
    protocol: ServicePortProtocol,
    policy: &ManifestResolutionPolicy,
    reserved_host_ports: &mut BTreeSet<u16>,
) -> Result<u16> {
    match host_port {
        ServiceManifestHostPort::Fixed(port) => {
            if !reserved_host_ports.insert(port) {
                bail!("host port is already reserved in this manifest or node: {port}");
            }
            Ok(port)
        }
        ServiceManifestHostPort::Keyword(value) => {
            let keyword = value.trim().to_ascii_lowercase();
            if keyword != "auto" {
                bail!("hostPort must be a number or the keyword: auto");
            }

            for port in iter_allowed_tcp_ports(policy) {
                if reserved_host_ports.contains(&port) {
                    continue;
                }
                if !is_host_port_available(port, protocol) {
                    continue;
                }
                reserved_host_ports.insert(port);
                return Ok(port);
            }

            bail!("failed to allocate host port automatically from allowed port policy")
        }
    }
}

fn iter_allowed_tcp_ports(policy: &ManifestResolutionPolicy) -> Vec<u16> {
    let mut ports = BTreeSet::new();
    for port in &policy.allowed_tcp_ports {
        ports.insert(*port);
    }
    for range in &policy.allowed_tcp_port_ranges {
        for port in range.start..=range.end {
            ports.insert(port);
        }
    }
    ports.into_iter().collect()
}

fn is_host_port_available(port: u16, protocol: ServicePortProtocol) -> bool {
    match protocol {
        ServicePortProtocol::Tcp => StdTcpListener::bind(("0.0.0.0", port)).is_ok(),
        ServicePortProtocol::Udp => StdUdpSocket::bind(("0.0.0.0", port)).is_ok(),
    }
}

fn resolve_manifest_path(
    path: &str,
    base_dir: &Path,
    fungi_home: &Path,
    app_home: &Path,
) -> PathBuf {
    let expanded = resolve_manifest_path_string(path, base_dir, fungi_home, app_home);
    PathBuf::from(expanded)
}

fn resolve_manifest_path_string(
    path: &str,
    base_dir: &Path,
    fungi_home: &Path,
    app_home: &Path,
) -> String {
    let fungi_home_value = fungi_home.to_string_lossy();
    let app_home_value = app_home.to_string_lossy();
    let expanded = path
        .replace("${FUNGI_HOME}", &fungi_home_value)
        .replace("$FUNGI_HOME", &fungi_home_value)
        .replace("${fungi_home}", &fungi_home_value)
        .replace("$fungi_home", &fungi_home_value)
        .replace("${APP_HOME}", &app_home_value)
        .replace("$APP_HOME", &app_home_value)
        .replace("${app_home}", &app_home_value)
        .replace("$app_home", &app_home_value);
    let resolved = PathBuf::from(&expanded);
    if resolved.is_absolute() {
        resolved.to_string_lossy().to_string()
    } else {
        base_dir.join(resolved).to_string_lossy().to_string()
    }
}

#[async_trait]
pub trait RuntimeProvider: Send + Sync {
    fn runtime_kind(&self) -> RuntimeKind;
    async fn deploy(&self, manifest: &ServiceManifest) -> Result<ServiceInstance>;
    async fn start(&self, handle: &str) -> Result<()>;
    async fn stop(&self, handle: &str) -> Result<()>;
    async fn remove(&self, handle: &str) -> Result<()>;
    async fn inspect(&self, handle: &str) -> Result<ServiceInstance>;
    async fn logs(&self, handle: &str, options: &ServiceLogsOptions) -> Result<ServiceLogs>;
}

#[derive(Clone)]
pub struct DockerRuntimeProvider {
    docker: DockerControl,
}

impl DockerRuntimeProvider {
    pub fn new(docker: DockerControl) -> Self {
        Self { docker }
    }
}

#[async_trait]
impl RuntimeProvider for DockerRuntimeProvider {
    fn runtime_kind(&self) -> RuntimeKind {
        RuntimeKind::Docker
    }

    async fn deploy(&self, manifest: &ServiceManifest) -> Result<ServiceInstance> {
        let spec = docker_spec_from_manifest(manifest)?;
        let details = self.docker.create_container(&spec).await?;
        Ok(map_docker_instance(details))
    }

    async fn start(&self, handle: &str) -> Result<()> {
        self.docker.start_container(handle).await
    }

    async fn stop(&self, handle: &str) -> Result<()> {
        self.docker.stop_container(handle).await
    }

    async fn remove(&self, handle: &str) -> Result<()> {
        self.docker.remove_container(handle).await
    }

    async fn inspect(&self, handle: &str) -> Result<ServiceInstance> {
        let details = self.docker.inspect_container(handle).await?;
        Ok(map_docker_instance(details))
    }

    async fn logs(&self, handle: &str, options: &ServiceLogsOptions) -> Result<ServiceLogs> {
        let logs = self
            .docker
            .container_logs(
                handle,
                &LogsOptions {
                    stdout: true,
                    stderr: true,
                    tail: options.tail.clone(),
                },
            )
            .await?;
        Ok(ServiceLogs {
            raw: logs.raw,
            text: logs.text,
        })
    }
}

#[derive(Clone)]
pub struct WasmtimeRuntimeProvider {
    runtime_root: PathBuf,
    launcher_path: PathBuf,
    allowed_host_paths: Arc<Mutex<Vec<PathBuf>>>,
    services: Arc<Mutex<HashMap<String, WasmtimeServiceState>>>,
}

struct WasmtimeServiceState {
    manifest: ServiceManifest,
    source_display: String,
    staged_component_path: PathBuf,
    service_dir: PathBuf,
    log_file_path: PathBuf,
    child: Option<Child>,
    last_exit_code: Option<i32>,
}

impl WasmtimeRuntimeProvider {
    pub fn new(
        runtime_root: PathBuf,
        launcher_path: PathBuf,
        allowed_host_paths: Vec<PathBuf>,
    ) -> Self {
        Self {
            runtime_root,
            launcher_path,
            allowed_host_paths: Arc::new(Mutex::new(allowed_host_paths)),
            services: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn update_allowed_host_paths(&self, allowed_host_paths: Vec<PathBuf>) {
        *self.allowed_host_paths.lock() = allowed_host_paths;
    }

    pub fn has_service(&self, handle: &str) -> bool {
        self.services.lock().contains_key(handle)
    }

    async fn restore(&self, manifest: &ServiceManifest) -> Result<()> {
        let allowed_host_paths = self.allowed_host_paths.lock().clone();
        let state =
            build_wasmtime_state(&self.runtime_root, &allowed_host_paths, manifest, false).await?;
        let mut services = self.services.lock();
        services.entry(manifest.name.clone()).or_insert(state);
        Ok(())
    }
}

#[async_trait]
impl RuntimeProvider for WasmtimeRuntimeProvider {
    fn runtime_kind(&self) -> RuntimeKind {
        RuntimeKind::Wasmtime
    }

    async fn deploy(&self, manifest: &ServiceManifest) -> Result<ServiceInstance> {
        let allowed_host_paths = self.allowed_host_paths.lock().clone();
        let state =
            build_wasmtime_state(&self.runtime_root, &allowed_host_paths, manifest, true).await?;

        {
            let mut services = self.services.lock();
            if services.contains_key(&manifest.name) {
                bail!("service already exists: {}", manifest.name);
            }
            services.insert(manifest.name.clone(), state);
        }

        self.inspect(&manifest.name).await
    }

    async fn start(&self, handle: &str) -> Result<()> {
        let mut services = self.services.lock();
        let state = services
            .get_mut(handle)
            .ok_or_else(|| anyhow::anyhow!("wasmtime service not found: {handle}"))?;

        refresh_child_state(state)?;
        if state.child.is_some() {
            bail!("wasmtime service is already running: {handle}");
        }

        let mut command = build_wasmtime_command(&self.launcher_path, state)?;
        let stdout = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&state.log_file_path)
            .with_context(|| {
                format!(
                    "Failed to open stdout log: {}",
                    state.log_file_path.display()
                )
            })?;
        let stderr = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&state.log_file_path)
            .with_context(|| {
                format!(
                    "Failed to open stderr log: {}",
                    state.log_file_path.display()
                )
            })?;

        command.stdout(Stdio::from(stdout));
        command.stderr(Stdio::from(stderr));

        let child = command
            .spawn()
            .context("Failed to spawn fungi WASI process")?;
        state.child = Some(child);
        state.last_exit_code = None;
        Ok(())
    }

    async fn stop(&self, handle: &str) -> Result<()> {
        let mut child = {
            let mut services = self.services.lock();
            let state = services
                .get_mut(handle)
                .ok_or_else(|| anyhow::anyhow!("wasmtime service not found: {handle}"))?;
            refresh_child_state(state)?;
            state.child.take()
        };

        let Some(mut child) = child.take() else {
            return Ok(());
        };

        child
            .kill()
            .await
            .context("Failed to kill fungi WASI process")?;
        let status = child
            .wait()
            .await
            .context("Failed to wait for fungi WASI process")?;

        let mut services = self.services.lock();
        let state = services
            .get_mut(handle)
            .ok_or_else(|| anyhow::anyhow!("wasmtime service not found after stop: {handle}"))?;
        state.last_exit_code = status.code();
        state.child = None;
        Ok(())
    }

    async fn remove(&self, handle: &str) -> Result<()> {
        self.stop(handle).await.ok();

        let state = {
            let mut services = self.services.lock();
            services.remove(handle)
        };

        let service_dir = state
            .map(|state| state.service_dir)
            .unwrap_or_else(|| self.runtime_root.join("wasmtime").join(handle));

        if service_dir.exists() {
            fs::remove_dir_all(&service_dir).with_context(|| {
                format!(
                    "Failed to remove runtime directory: {}",
                    service_dir.display()
                )
            })?;
        }
        Ok(())
    }

    async fn inspect(&self, handle: &str) -> Result<ServiceInstance> {
        let mut services = self.services.lock();
        let state = services
            .get_mut(handle)
            .ok_or_else(|| anyhow::anyhow!("wasmtime service not found: {handle}"))?;
        refresh_child_state(state)?;
        Ok(map_wasmtime_instance(handle, state))
    }

    async fn logs(&self, handle: &str, options: &ServiceLogsOptions) -> Result<ServiceLogs> {
        let log_file_path = {
            let services = self.services.lock();
            services
                .get(handle)
                .ok_or_else(|| anyhow::anyhow!("wasmtime service not found: {handle}"))?
                .log_file_path
                .clone()
        };

        let mut raw = Vec::new();
        if log_file_path.exists() {
            fs::File::open(&log_file_path)
                .and_then(|mut file| file.read_to_end(&mut raw))
                .with_context(|| format!("Failed to read log file: {}", log_file_path.display()))?;
        }

        let text = String::from_utf8_lossy(&raw).to_string();
        Ok(ServiceLogs {
            raw,
            text: tail_lines(&text, options.tail.as_deref()),
        })
    }
}

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
        docker: Option<DockerControl>,
        service_state_file: PathBuf,
        allowed_host_paths: Vec<PathBuf>,
        wasmtime_enabled: bool,
    ) -> Result<Self> {
        Ok(Self {
            docker: docker.map(DockerRuntimeProvider::new),
            wasmtime: WasmtimeRuntimeProvider::new(runtime_root, launcher_path, allowed_host_paths),
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

    pub async fn deploy(&self, manifest: &ServiceManifest) -> Result<ServiceInstance> {
        self.ensure_runtime_enabled(manifest.runtime)?;
        {
            let services = self.service_index.lock();
            if services.contains_key(&manifest.name) {
                bail!("service already exists: {}", manifest.name);
            }
        }

        let instance = match manifest.runtime {
            RuntimeKind::Docker => self.docker_provider()?.deploy(manifest).await,
            RuntimeKind::Wasmtime => self.wasmtime.deploy(manifest).await,
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

    pub async fn deploy_manifest_yaml(
        &self,
        content: &str,
        base_dir: &Path,
        fungi_home: &Path,
        policy: &ManifestResolutionPolicy,
    ) -> Result<ServiceInstance> {
        let manifest = self.resolve_manifest_yaml(content, base_dir, fungi_home, policy)?;
        self.deploy(&manifest).await
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

    pub async fn start(&self, runtime: RuntimeKind, handle: &str) -> Result<()> {
        self.ensure_runtime_enabled(runtime)?;
        self.ensure_runtime_service(runtime, handle).await?;
        match runtime {
            RuntimeKind::Docker => self.docker_provider()?.start(handle).await,
            RuntimeKind::Wasmtime => self.wasmtime.start(handle).await,
        }?;
        self.set_desired_state(handle, DesiredServiceState::Running)
    }

    pub async fn stop(&self, runtime: RuntimeKind, handle: &str) -> Result<()> {
        let _ = self.ensure_runtime_service(runtime, handle).await;
        match runtime {
            RuntimeKind::Docker => self.docker_provider()?.stop(handle).await,
            RuntimeKind::Wasmtime => self.wasmtime.stop(handle).await,
        }?;
        self.set_desired_state(handle, DesiredServiceState::Stopped)
    }

    pub async fn remove(&self, runtime: RuntimeKind, handle: &str) -> Result<()> {
        match runtime {
            RuntimeKind::Docker => self.docker_provider()?.remove(handle).await,
            RuntimeKind::Wasmtime => self.wasmtime.remove(handle).await,
        }?;
        self.service_index.lock().remove(handle);
        self.service_manifests.lock().remove(handle);
        self.service_state.lock().remove_service(handle)?;
        Ok(())
    }

    pub async fn start_by_handle(&self, handle: &str) -> Result<()> {
        let runtime = self.resolve_runtime(handle)?;
        self.start(runtime, handle).await
    }

    pub fn get_service_manifest(&self, handle: &str) -> Option<ServiceManifest> {
        self.service_manifests.lock().get(handle).cloned()
    }

    pub async fn stop_by_handle(&self, handle: &str) -> Result<()> {
        let runtime = self.resolve_runtime(handle)?;
        self.stop(runtime, handle).await
    }

    pub async fn remove_by_handle(&self, handle: &str) -> Result<()> {
        let runtime = self.resolve_runtime(handle)?;
        self.remove(runtime, handle).await
    }

    pub async fn inspect_by_handle(&self, handle: &str) -> Result<ServiceInstance> {
        let runtime = self.resolve_runtime(handle)?;
        self.inspect(runtime, handle).await
    }

    pub async fn logs_by_handle(
        &self,
        handle: &str,
        options: &ServiceLogsOptions,
    ) -> Result<ServiceLogs> {
        let runtime = self.resolve_runtime(handle)?;
        self.logs(runtime, handle, options).await
    }

    pub async fn list_exposed_services(&self) -> Result<Vec<DiscoveredService>> {
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

            services.push(DiscoveredService {
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
                    .map(|endpoint| DiscoveredServiceEndpoint {
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

    pub async fn inspect(&self, runtime: RuntimeKind, handle: &str) -> Result<ServiceInstance> {
        if let Err(error) = self.ensure_runtime_service(runtime, handle).await {
            if let Some(manifest) = self.get_service_manifest(handle) {
                log::warn!(
                    "Failed to restore service '{}' for inspect: {}",
                    handle,
                    error
                );
                return Ok(missing_instance_from_manifest(&manifest));
            }
            return Err(error);
        }

        let instance = match runtime {
            RuntimeKind::Docker => self.docker_provider()?.inspect(handle).await,
            RuntimeKind::Wasmtime => self.wasmtime.inspect(handle).await,
        }?;

        if let Some(manifest) = self.get_service_manifest(handle) {
            Ok(enrich_instance_from_manifest(instance, &manifest))
        } else {
            Ok(instance)
        }
    }

    pub async fn logs(
        &self,
        runtime: RuntimeKind,
        handle: &str,
        options: &ServiceLogsOptions,
    ) -> Result<ServiceLogs> {
        self.ensure_runtime_enabled(runtime)?;
        self.ensure_runtime_service(runtime, handle).await?;
        match runtime {
            RuntimeKind::Docker => self.docker_provider()?.logs(handle, options).await,
            RuntimeKind::Wasmtime => self.wasmtime.logs(handle, options).await,
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

    async fn ensure_runtime_service(&self, runtime: RuntimeKind, handle: &str) -> Result<()> {
        if runtime != RuntimeKind::Wasmtime || self.wasmtime.has_service(handle) {
            return Ok(());
        }

        let manifest = self
            .service_manifests
            .lock()
            .get(handle)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("service not found: {handle}"))?;
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

    fn set_desired_state(&self, handle: &str, desired_state: DesiredServiceState) -> Result<()> {
        self.service_state
            .lock()
            .set_desired_state(handle, desired_state)
    }

    fn resolve_runtime(&self, handle: &str) -> Result<RuntimeKind> {
        self.service_index
            .lock()
            .get(handle)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("service not found: {handle}"))
    }

    fn reserved_host_ports(&self) -> BTreeSet<u16> {
        self.service_manifests
            .lock()
            .values()
            .flat_map(|manifest| manifest.ports.iter().map(|port| port.host_port))
            .collect()
    }
}

fn docker_spec_from_manifest(manifest: &ServiceManifest) -> Result<ContainerSpec> {
    if manifest.runtime != RuntimeKind::Docker {
        bail!("service manifest runtime does not match docker provider")
    }

    let ServiceSource::Docker { image } = &manifest.source else {
        bail!("docker runtime requires a docker image source")
    };

    Ok(ContainerSpec {
        name: Some(manifest.name.clone()),
        image: image.clone(),
        env: manifest.env.clone(),
        mounts: manifest
            .mounts
            .iter()
            .map(|mount| fungi_docker_agent::BindMount {
                host_path: mount.host_path.clone(),
                container_path: mount.runtime_path.clone(),
            })
            .collect(),
        ports: manifest
            .ports
            .iter()
            .map(|port| fungi_docker_agent::PortBinding {
                host_port: port.host_port,
                container_port: port.service_port,
                protocol: match port.protocol {
                    ServicePortProtocol::Tcp => PortProtocol::Tcp,
                    ServicePortProtocol::Udp => PortProtocol::Udp,
                },
            })
            .collect(),
        command: manifest.command.clone(),
        entrypoint: manifest.entrypoint.clone(),
        working_dir: manifest.working_dir.clone(),
        labels: manifest.labels.clone(),
    })
}

fn ensure_wasmtime_manifest(manifest: &ServiceManifest) -> Result<()> {
    if manifest.runtime != RuntimeKind::Wasmtime {
        bail!("service manifest runtime does not match wasmtime provider")
    }

    match &manifest.source {
        ServiceSource::WasmtimeFile { component } => {
            if component.as_os_str().is_empty() {
                bail!("wasmtime component path must not be empty");
            }
            Ok(())
        }
        ServiceSource::WasmtimeUrl { url } => {
            if url.trim().is_empty() {
                bail!("wasmtime source url must not be empty");
            }
            Ok(())
        }
        ServiceSource::Docker { .. } => bail!("wasmtime runtime requires a wasm component source"),
    }
}

fn ensure_wasmtime_mount_dirs(manifest: &ServiceManifest) -> Result<()> {
    for mount in &manifest.mounts {
        fs::create_dir_all(&mount.host_path).with_context(|| {
            format!(
                "Failed to create wasmtime host mount directory: {}",
                mount.host_path.display()
            )
        })?;
    }
    Ok(())
}

fn validate_allowed_host_paths(
    manifest: &ServiceManifest,
    allowed_host_paths: &[PathBuf],
) -> Result<()> {
    let normalized_roots = allowed_host_paths
        .iter()
        .map(|path| normalize_absolute_path(path))
        .collect::<Result<Vec<_>>>()?;

    for mount in &manifest.mounts {
        let host_path = normalize_absolute_path(&mount.host_path)?;
        let allowed = normalized_roots
            .iter()
            .any(|allowed_root| host_path.starts_with(allowed_root));
        if !allowed {
            bail!(
                "wasmtime host path is outside allowed roots: {}",
                mount.host_path.display()
            );
        }
    }

    Ok(())
}

async fn stage_wasmtime_component(
    manifest: &ServiceManifest,
    service_dir: &Path,
) -> Result<PathBuf> {
    let target_path = service_dir.join("component.wasm");
    match &manifest.source {
        ServiceSource::WasmtimeFile { component } => {
            fs::copy(component, &target_path).with_context(|| {
                format!(
                    "Failed to copy WASI component from {} to {}",
                    component.display(),
                    target_path.display()
                )
            })?;
        }
        ServiceSource::WasmtimeUrl { url } => {
            let response = reqwest::get(url)
                .await
                .with_context(|| format!("Failed to download WASI component from {url}"))?
                .error_for_status()
                .with_context(|| format!("WASI component download returned error for {url}"))?;
            let bytes = response
                .bytes()
                .await
                .with_context(|| format!("Failed to read WASI download body from {url}"))?;
            fs::write(&target_path, &bytes).with_context(|| {
                format!(
                    "Failed to write staged WASI component: {}",
                    target_path.display()
                )
            })?;
        }
        ServiceSource::Docker { .. } => bail!("invalid wasmtime source type"),
    }
    Ok(target_path)
}

async fn build_wasmtime_state(
    runtime_root: &Path,
    allowed_host_paths: &[PathBuf],
    manifest: &ServiceManifest,
    restage_component: bool,
) -> Result<WasmtimeServiceState> {
    ensure_wasmtime_manifest(manifest)?;
    validate_allowed_host_paths(manifest, allowed_host_paths)?;

    let service_dir = runtime_root.join("wasmtime").join(&manifest.name);
    fs::create_dir_all(&service_dir).with_context(|| {
        format!(
            "Failed to create runtime directory: {}",
            service_dir.display()
        )
    })?;
    ensure_wasmtime_mount_dirs(manifest)?;

    let staged_component_path = service_dir.join("component.wasm");
    let staged_component_path = if restage_component || !staged_component_path.exists() {
        stage_wasmtime_component(manifest, &service_dir).await?
    } else {
        staged_component_path
    };

    let log_file_path = service_dir.join("runtime.log");
    if !log_file_path.exists() {
        fs::File::create(&log_file_path)
            .with_context(|| format!("Failed to create log file: {}", log_file_path.display()))?;
    }

    Ok(WasmtimeServiceState {
        manifest: manifest.clone(),
        source_display: source_display(&manifest.source),
        staged_component_path,
        service_dir,
        log_file_path,
        child: None,
        last_exit_code: None,
    })
}

fn build_wasmtime_command(launcher_path: &Path, state: &WasmtimeServiceState) -> Result<Command> {
    let mut command = Command::new(launcher_path);
    command.kill_on_drop(true);

    let is_http_service = !state.manifest.ports.is_empty();
    if is_http_service {
        let port = state.manifest.ports[0].host_port;
        command.arg("serve");
        command.arg(format!("--addr=127.0.0.1:{port}"));
        command.arg("-Scli");
    } else {
        command.arg("run");
    }

    for mount in &state.manifest.mounts {
        command.arg("--dir");
        let guest = mount.runtime_path.trim().trim_start_matches('/');
        if guest.is_empty() {
            command.arg(mount.host_path.as_os_str());
        } else {
            command.arg(format!("{}::{}", mount.host_path.display(), guest));
        }
    }

    command.arg(&state.staged_component_path);
    for arg in &state.manifest.command {
        command.arg(arg);
    }
    if let Some(working_dir) = &state.manifest.working_dir {
        command.current_dir(working_dir);
    } else {
        command.current_dir(&state.service_dir);
    }
    command.envs(&state.manifest.env);
    Ok(command)
}

fn refresh_child_state(state: &mut WasmtimeServiceState) -> Result<()> {
    if let Some(child) = state.child.as_mut()
        && let Some(status) = child
            .try_wait()
            .context("Failed to query fungi WASI process status")?
    {
        state.last_exit_code = status.code();
        state.child = None;
    }
    Ok(())
}

fn map_docker_instance(details: fungi_docker_agent::ContainerDetails) -> ServiceInstance {
    ServiceInstance {
        runtime: RuntimeKind::Docker,
        handle: details.id.clone(),
        name: details.name,
        source: details.image,
        labels: details.labels,
        ports: Vec::new(),
        exposed_endpoints: Vec::new(),
        status: ServiceStatus {
            state: details.state.status,
            running: details.state.running,
        },
    }
}

fn map_wasmtime_instance(handle: &str, state: &WasmtimeServiceState) -> ServiceInstance {
    let running = state.child.is_some();
    let status = if running {
        if state.manifest.ports.is_empty() {
            "running".to_string()
        } else {
            "serving".to_string()
        }
    } else if let Some(code) = state.last_exit_code {
        format!("exited({code})")
    } else {
        "created".to_string()
    };

    ServiceInstance {
        runtime: RuntimeKind::Wasmtime,
        handle: handle.to_string(),
        name: state.manifest.name.clone(),
        source: state.source_display.clone(),
        labels: state.manifest.labels.clone(),
        ports: Vec::new(),
        exposed_endpoints: Vec::new(),
        status: ServiceStatus {
            state: status,
            running,
        },
    }
}

fn missing_instance_from_manifest(manifest: &ServiceManifest) -> ServiceInstance {
    ServiceInstance {
        runtime: manifest.runtime,
        handle: manifest.name.clone(),
        name: manifest.name.clone(),
        source: source_display(&manifest.source),
        labels: manifest.labels.clone(),
        ports: manifest.ports.clone(),
        exposed_endpoints: service_expose_endpoint_bindings(manifest),
        status: ServiceStatus {
            state: "missing".to_string(),
            running: false,
        },
    }
}

fn enrich_instance_from_manifest(
    mut instance: ServiceInstance,
    manifest: &ServiceManifest,
) -> ServiceInstance {
    instance.ports = manifest.ports.clone();
    instance.exposed_endpoints = service_expose_endpoint_bindings(manifest);
    instance
}

fn source_display(source: &ServiceSource) -> String {
    match source {
        ServiceSource::Docker { image } => image.clone(),
        ServiceSource::WasmtimeFile { component } => component.display().to_string(),
        ServiceSource::WasmtimeUrl { url } => url.clone(),
    }
}

fn normalize_absolute_path(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute() {
        bail!("host path must be absolute: {}", path.display());
    }

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::Normal(part) => normalized.push(part),
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Prefix(_) => normalized.push(component.as_os_str()),
        }
    }
    Ok(normalized)
}

fn tail_lines(text: &str, tail: Option<&str>) -> String {
    let Some(tail) = tail else {
        return text.to_string();
    };
    let Ok(count) = tail.parse::<usize>() else {
        return text.to_string();
    };
    let lines = text.lines().collect::<Vec<_>>();
    let start = lines.len().saturating_sub(count);
    let mut output = lines[start..].join("\n");
    if text.ends_with('\n') && !output.is_empty() {
        output.push('\n');
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Write, net::SocketAddr, os::unix::fs::PermissionsExt};
    use tempfile::TempDir;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
        time::{Duration, sleep},
    };

    #[test]
    fn docker_manifest_maps_to_container_spec() {
        let manifest = ServiceManifest {
            name: "filebrowser".into(),
            runtime: RuntimeKind::Docker,
            source: ServiceSource::Docker {
                image: "filebrowser/filebrowser:latest".into(),
            },
            expose: None,
            env: BTreeMap::from([(String::from("FB_NOAUTH"), String::from("true"))]),
            mounts: vec![ServiceMount {
                host_path: PathBuf::from("/tmp/fungi/data"),
                runtime_path: "/srv".into(),
            }],
            ports: vec![ServicePort {
                name: None,
                host_port: 8080,
                service_port: 80,
                protocol: ServicePortProtocol::Tcp,
            }],
            command: vec!["serve".into()],
            entrypoint: Vec::new(),
            working_dir: None,
            labels: BTreeMap::new(),
        };

        let spec = docker_spec_from_manifest(&manifest).unwrap();
        assert_eq!(spec.name.as_deref(), Some("filebrowser"));
        assert_eq!(spec.image, "filebrowser/filebrowser:latest");
        assert_eq!(spec.ports[0].host_port, 8080);
    }

    #[test]
    fn ensure_wasmtime_mount_dirs_creates_missing_host_paths() {
        let temp_dir = TempDir::new().unwrap();
        let mount_path = temp_dir.path().join("nested/data");
        let manifest = ServiceManifest {
            name: "mount-test".into(),
            runtime: RuntimeKind::Wasmtime,
            source: ServiceSource::WasmtimeFile {
                component: temp_dir.path().join("demo.wasm"),
            },
            expose: None,
            env: BTreeMap::new(),
            mounts: vec![ServiceMount {
                host_path: mount_path.clone(),
                runtime_path: "data".into(),
            }],
            ports: Vec::new(),
            command: Vec::new(),
            entrypoint: Vec::new(),
            working_dir: None,
            labels: BTreeMap::new(),
        };

        ensure_wasmtime_mount_dirs(&manifest).unwrap();
        assert!(mount_path.is_dir());
    }

    #[test]
    fn docker_manifest_rejects_wrong_source_type() {
        let manifest = ServiceManifest {
            name: "bad".into(),
            runtime: RuntimeKind::Docker,
            source: ServiceSource::WasmtimeFile {
                component: PathBuf::from("/tmp/app.wasm"),
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

        assert!(docker_spec_from_manifest(&manifest).is_err());
    }

    #[tokio::test]
    async fn wasmtime_provider_runs_fake_launcher_and_collects_logs() {
        let temp_dir = TempDir::new().unwrap();
        let launcher = create_fake_launcher(temp_dir.path()).unwrap();
        let component = temp_dir.path().join("demo.wasm");
        fs::write(&component, b"wasm-bytes").unwrap();

        let provider = WasmtimeRuntimeProvider::new(
            temp_dir.path().join("runtime"),
            launcher,
            vec![temp_dir.path().to_path_buf()],
        );
        let manifest = ServiceManifest {
            name: "demo-service".into(),
            runtime: RuntimeKind::Wasmtime,
            source: ServiceSource::WasmtimeFile {
                component: component.clone(),
            },
            expose: None,
            env: BTreeMap::new(),
            mounts: vec![ServiceMount {
                host_path: temp_dir.path().join("data"),
                runtime_path: "data".into(),
            }],
            ports: vec![ServicePort {
                name: None,
                host_port: 18081,
                service_port: 8081,
                protocol: ServicePortProtocol::Tcp,
            }],
            command: Vec::new(),
            entrypoint: Vec::new(),
            working_dir: None,
            labels: BTreeMap::new(),
        };

        provider.deploy(&manifest).await.unwrap();
        let created = provider.inspect("demo-service").await.unwrap();
        assert_eq!(created.status.state, "created");

        provider.start("demo-service").await.unwrap();
        sleep(Duration::from_millis(150)).await;

        let running = provider.inspect("demo-service").await.unwrap();
        assert!(running.status.running);

        let mut logs = ServiceLogs {
            raw: Vec::new(),
            text: String::new(),
        };
        for _ in 0..10 {
            logs = provider
                .logs(
                    "demo-service",
                    &ServiceLogsOptions {
                        tail: Some("10".into()),
                    },
                )
                .await
                .unwrap();
            if logs.text.contains("fake-launcher") {
                break;
            }
            sleep(Duration::from_millis(100)).await;
        }
        assert!(logs.text.contains("fake-launcher"));
        assert!(logs.text.contains("serve"));

        provider.stop("demo-service").await.unwrap();
        let stopped = provider.inspect("demo-service").await.unwrap();
        assert!(!stopped.status.running);

        provider.remove("demo-service").await.unwrap();
        assert!(provider.inspect("demo-service").await.is_err());
    }

    #[test]
    fn manifest_document_supports_app_home_and_auto_host_port() {
        let yaml = r#"
apiVersion: fungi.dev/v1alpha1
kind: ServiceManifest
metadata:
    name: filebrowser
spec:
    runtime: docker
    source:
        image: filebrowser/filebrowser:latest
    mounts:
        - hostPath: ${APP_HOME}/data
            runtimePath: /srv
    ports:
        - hostPort: auto
            servicePort: 80
            protocol: tcp
"#;

        let fungi_home = PathBuf::from("/tmp/fungi-home");
        let manifest = parse_service_manifest_yaml_with_policy(
            yaml,
            Path::new("."),
            &fungi_home,
            &ManifestResolutionPolicy {
                allowed_tcp_ports: vec![28080],
                allowed_tcp_port_ranges: Vec::new(),
            },
            &BTreeSet::new(),
        )
        .unwrap();

        assert_eq!(
            manifest.mounts[0].host_path,
            fungi_home.join("services/filebrowser/data")
        );
        assert_eq!(manifest.ports[0].host_port, 28080);
    }

    #[tokio::test]
    async fn wasmtime_provider_downloads_remote_component() {
        let temp_dir = TempDir::new().unwrap();
        let launcher = create_fake_launcher(temp_dir.path()).unwrap();
        let server = spawn_http_server(b"downloaded-wasm".to_vec()).await;

        let provider = WasmtimeRuntimeProvider::new(
            temp_dir.path().join("runtime"),
            launcher,
            vec![temp_dir.path().to_path_buf()],
        );
        let manifest = ServiceManifest {
            name: "download-service".into(),
            runtime: RuntimeKind::Wasmtime,
            source: ServiceSource::WasmtimeUrl {
                url: server.url.clone(),
            },
            expose: None,
            env: BTreeMap::new(),
            mounts: Vec::new(),
            ports: Vec::new(),
            command: vec!["--help".into()],
            entrypoint: Vec::new(),
            working_dir: None,
            labels: BTreeMap::new(),
        };

        let deployed = provider.deploy(&manifest).await.unwrap();
        assert_eq!(deployed.status.state, "created");
        assert!(
            temp_dir
                .path()
                .join("runtime/wasmtime/download-service/component.wasm")
                .exists()
        );
        drop(server);
    }

    #[test]
    fn parse_manifest_expose_defaults_service_identity() {
        let yaml = r#"
apiVersion: fungi.dev/v1alpha1
kind: ServiceManifest
metadata:
    name: filebrowser
spec:
    runtime: docker
    expose:
        enabled: true
        transport:
            kind: tcp
        usage:
            kind: web
            path: /
    source:
        image: filebrowser/filebrowser:latest
"#;

        let manifest =
            parse_service_manifest_yaml(yaml, Path::new("/tmp"), Path::new("/tmp")).unwrap();
        let expose = manifest.expose.expect("expected expose config");
        assert_eq!(expose.service_id, "filebrowser");
        assert_eq!(expose.display_name, "filebrowser");
        assert_eq!(expose.transport.kind, ServiceExposeTransportKind::Tcp);
        let usage = expose.usage.expect("expected usage config");
        assert_eq!(usage.kind, ServiceExposeUsageKind::Web);
        assert_eq!(usage.path.as_deref(), Some("/"));
    }

    #[test]
    fn parse_manifest_expose_disabled_returns_none() {
        let yaml = r#"
apiVersion: fungi.dev/v1alpha1
kind: ServiceManifest
metadata:
    name: raw-service
spec:
    runtime: docker
    expose:
        enabled: false
        transport:
            kind: tcp
    source:
        image: example/raw:latest
"#;

        let manifest =
            parse_service_manifest_yaml(yaml, Path::new("/tmp"), Path::new("/tmp")).unwrap();
        assert!(manifest.expose.is_none());
    }

    fn create_fake_launcher(dir: &Path) -> Result<PathBuf> {
        let launcher = dir.join("fake-fungi.sh");
        let script = r#"#!/bin/sh
echo fake-launcher "$@"
sleep 30
"#;
        let mut file = fs::File::create(&launcher)?;
        file.write_all(script.as_bytes())?;
        let mut permissions = fs::metadata(&launcher)?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&launcher, permissions)?;
        Ok(launcher)
    }

    struct TestHttpServer {
        url: String,
    }

    async fn spawn_http_server(body: Vec<u8>) -> TestHttpServer {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0_u8; 1024];
            let _ = socket.read(&mut buffer).await.unwrap();
            let mut response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .into_bytes();
            response.extend_from_slice(&body);
            socket.write_all(&response).await.unwrap();
        });

        TestHttpServer {
            url: format!("http://{addr}/app.wasm"),
        }
    }
}
