use std::{
    collections::{BTreeMap, HashMap},
    fs::{self, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
};

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use fungi_docker_agent::{ContainerSpec, LogsOptions, PortProtocol};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tokio::process::{Child, Command};

use crate::controls::DockerControl;

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
pub struct ServiceMount {
    pub host_path: PathBuf,
    pub runtime_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServicePort {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInstance {
    pub runtime: RuntimeKind,
    pub handle: String,
    pub name: String,
    pub source: String,
    pub labels: BTreeMap<String, String>,
    pub status: ServiceStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceStatus {
    pub state: String,
    pub running: bool,
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
    pub host_port: u16,
    #[serde(rename = "servicePort")]
    pub service_port: u16,
    pub protocol: ServicePortProtocol,
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
    let document: ServiceManifestDocument =
        serde_yaml::from_str(content).context("Failed to parse service manifest YAML")?;
    document.into_service_manifest(base_dir, fungi_home)
}

impl ServiceManifestDocument {
    pub fn into_service_manifest(
        self,
        base_dir: &Path,
        fungi_home: &Path,
    ) -> Result<ServiceManifest> {
        if self.kind != "ServiceManifest" {
            bail!("Unsupported manifest kind: {}", self.kind);
        }

        let runtime = self.spec.runtime;
        let source = match runtime {
            RuntimeKind::Docker => {
                let Some(image) = self.spec.source.image else {
                    bail!("docker service manifest requires spec.source.image");
                };
                ServiceSource::Docker { image }
            }
            RuntimeKind::Wasmtime => match (self.spec.source.file, self.spec.source.url) {
                (Some(file), None) => ServiceSource::WasmtimeFile {
                    component: resolve_manifest_path(&file, base_dir, fungi_home),
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

        Ok(ServiceManifest {
            name: self.metadata.name,
            runtime,
            source,
            env: self.spec.env,
            mounts: self
                .spec
                .mounts
                .into_iter()
                .map(|mount| ServiceMount {
                    host_path: resolve_manifest_path(&mount.host_path, base_dir, fungi_home),
                    runtime_path: mount.runtime_path,
                })
                .collect(),
            ports: self
                .spec
                .ports
                .into_iter()
                .map(|port| ServicePort {
                    host_port: port.host_port,
                    service_port: port.service_port,
                    protocol: port.protocol,
                })
                .collect(),
            command: self.spec.command,
            entrypoint: self.spec.entrypoint,
            working_dir: self
                .spec
                .working_dir
                .map(|value| resolve_manifest_path_string(&value, base_dir, fungi_home)),
            labels: self.metadata.labels,
        })
    }
}

fn resolve_manifest_path(path: &str, base_dir: &Path, fungi_home: &Path) -> PathBuf {
    let expanded = resolve_manifest_path_string(path, base_dir, fungi_home);
    PathBuf::from(expanded)
}

fn resolve_manifest_path_string(path: &str, base_dir: &Path, fungi_home: &Path) -> String {
    let fungi_home_value = fungi_home.to_string_lossy();
    let expanded = path
        .replace("${FUNGI_HOME}", &fungi_home_value)
        .replace("$FUNGI_HOME", &fungi_home_value);
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
    pub fn new(runtime_root: PathBuf, launcher_path: PathBuf) -> Self {
        Self {
            runtime_root,
            launcher_path,
            services: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl RuntimeProvider for WasmtimeRuntimeProvider {
    fn runtime_kind(&self) -> RuntimeKind {
        RuntimeKind::Wasmtime
    }

    async fn deploy(&self, manifest: &ServiceManifest) -> Result<ServiceInstance> {
        ensure_wasmtime_manifest(manifest)?;

        let service_dir = self.runtime_root.join("wasmtime").join(&manifest.name);
        fs::create_dir_all(&service_dir).with_context(|| {
            format!(
                "Failed to create runtime directory: {}",
                service_dir.display()
            )
        })?;
        ensure_wasmtime_mount_dirs(manifest)?;

        let staged_component_path = stage_wasmtime_component(manifest, &service_dir).await?;
        let log_file_path = service_dir.join("runtime.log");
        if !log_file_path.exists() {
            fs::File::create(&log_file_path).with_context(|| {
                format!("Failed to create log file: {}", log_file_path.display())
            })?;
        }

        let state = WasmtimeServiceState {
            manifest: manifest.clone(),
            source_display: source_display(&manifest.source),
            staged_component_path,
            service_dir,
            log_file_path,
            child: None,
            last_exit_code: None,
        };

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

        let Some(state) = state else {
            bail!("wasmtime service not found: {handle}");
        };

        if state.service_dir.exists() {
            fs::remove_dir_all(&state.service_dir).with_context(|| {
                format!(
                    "Failed to remove runtime directory: {}",
                    state.service_dir.display()
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
}

impl RuntimeControl {
    pub fn new(
        runtime_root: PathBuf,
        launcher_path: PathBuf,
        docker: Option<DockerControl>,
    ) -> Self {
        Self {
            docker: docker.map(DockerRuntimeProvider::new),
            wasmtime: WasmtimeRuntimeProvider::new(runtime_root, launcher_path),
        }
    }

    pub fn with_wasmtime_provider(
        wasmtime: WasmtimeRuntimeProvider,
        docker: Option<DockerControl>,
    ) -> Self {
        Self {
            docker: docker.map(DockerRuntimeProvider::new),
            wasmtime,
        }
    }

    pub fn supports(&self, runtime: RuntimeKind) -> bool {
        match runtime {
            RuntimeKind::Docker => self.docker.is_some(),
            RuntimeKind::Wasmtime => true,
        }
    }

    pub async fn deploy(&self, manifest: &ServiceManifest) -> Result<ServiceInstance> {
        match manifest.runtime {
            RuntimeKind::Docker => self.docker_provider()?.deploy(manifest).await,
            RuntimeKind::Wasmtime => self.wasmtime.deploy(manifest).await,
        }
    }

    pub async fn start(&self, runtime: RuntimeKind, handle: &str) -> Result<()> {
        match runtime {
            RuntimeKind::Docker => self.docker_provider()?.start(handle).await,
            RuntimeKind::Wasmtime => self.wasmtime.start(handle).await,
        }
    }

    pub async fn stop(&self, runtime: RuntimeKind, handle: &str) -> Result<()> {
        match runtime {
            RuntimeKind::Docker => self.docker_provider()?.stop(handle).await,
            RuntimeKind::Wasmtime => self.wasmtime.stop(handle).await,
        }
    }

    pub async fn remove(&self, runtime: RuntimeKind, handle: &str) -> Result<()> {
        match runtime {
            RuntimeKind::Docker => self.docker_provider()?.remove(handle).await,
            RuntimeKind::Wasmtime => self.wasmtime.remove(handle).await,
        }
    }

    pub async fn inspect(&self, runtime: RuntimeKind, handle: &str) -> Result<ServiceInstance> {
        match runtime {
            RuntimeKind::Docker => self.docker_provider()?.inspect(handle).await,
            RuntimeKind::Wasmtime => self.wasmtime.inspect(handle).await,
        }
    }

    pub async fn logs(
        &self,
        runtime: RuntimeKind,
        handle: &str,
        options: &ServiceLogsOptions,
    ) -> Result<ServiceLogs> {
        match runtime {
            RuntimeKind::Docker => self.docker_provider()?.logs(handle, options).await,
            RuntimeKind::Wasmtime => self.wasmtime.logs(handle, options).await,
        }
    }

    fn docker_provider(&self) -> Result<&DockerRuntimeProvider> {
        self.docker
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("docker runtime is not enabled in config"))
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
        status: ServiceStatus {
            state: status,
            running,
        },
    }
}

fn source_display(source: &ServiceSource) -> String {
    match source {
        ServiceSource::Docker { image } => image.clone(),
        ServiceSource::WasmtimeFile { component } => component.display().to_string(),
        ServiceSource::WasmtimeUrl { url } => url.clone(),
    }
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
            env: BTreeMap::from([(String::from("FB_NOAUTH"), String::from("true"))]),
            mounts: vec![ServiceMount {
                host_path: PathBuf::from("/tmp/fungi/data"),
                runtime_path: "/srv".into(),
            }],
            ports: vec![ServicePort {
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

        let provider = WasmtimeRuntimeProvider::new(temp_dir.path().join("runtime"), launcher);
        let manifest = ServiceManifest {
            name: "demo-service".into(),
            runtime: RuntimeKind::Wasmtime,
            source: ServiceSource::WasmtimeFile {
                component: component.clone(),
            },
            env: BTreeMap::new(),
            mounts: vec![ServiceMount {
                host_path: temp_dir.path().join("data"),
                runtime_path: "data".into(),
            }],
            ports: vec![ServicePort {
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

    #[tokio::test]
    async fn wasmtime_provider_downloads_remote_component() {
        let temp_dir = TempDir::new().unwrap();
        let launcher = create_fake_launcher(temp_dir.path()).unwrap();
        let server = spawn_http_server(b"downloaded-wasm".to_vec()).await;

        let provider = WasmtimeRuntimeProvider::new(temp_dir.path().join("runtime"), launcher);
        let manifest = ServiceManifest {
            name: "download-service".into(),
            runtime: RuntimeKind::Wasmtime,
            source: ServiceSource::WasmtimeUrl {
                url: server.url.clone(),
            },
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
