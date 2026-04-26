use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
};

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use fungi_docker_agent::LogsOptions;
use parking_lot::Mutex;
use tokio::process::Child;

use crate::controls::DockerControl;

use super::{
    helpers::{
        build_wasmtime_command, build_wasmtime_state, docker_spec_from_manifest,
        ensure_manifest_mount_dirs, map_docker_instance, map_wasmtime_instance,
        refresh_child_state, tail_lines,
    },
    model::*,
};

#[async_trait]
pub trait RuntimeProvider: Send + Sync {
    fn runtime_kind(&self) -> RuntimeKind;
    async fn pull(&self, manifest: &ServiceManifest) -> Result<ServiceInstance>;
    async fn start(&self, name: &str) -> Result<()>;
    async fn stop(&self, name: &str) -> Result<()>;
    async fn remove(&self, name: &str) -> Result<()>;
    async fn inspect(&self, name: &str) -> Result<ServiceInstance>;
    async fn logs(&self, name: &str, options: &ServiceLogsOptions) -> Result<ServiceLogs>;
}

pub const fn wasmtime_runtime_supported() -> bool {
    !cfg!(target_os = "android")
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

    async fn pull(&self, manifest: &ServiceManifest) -> Result<ServiceInstance> {
        ensure_manifest_mount_dirs(manifest)?;
        let spec = docker_spec_from_manifest(manifest)?;
        let details = self.docker.create_container(&spec).await?;
        Ok(map_docker_instance(details))
    }

    async fn start(&self, name: &str) -> Result<()> {
        self.docker.start_container(name).await
    }

    async fn stop(&self, name: &str) -> Result<()> {
        self.docker.stop_container(name).await
    }

    async fn remove(&self, name: &str) -> Result<()> {
        self.docker.remove_container(name).await
    }

    async fn inspect(&self, name: &str) -> Result<ServiceInstance> {
        let details = self.docker.inspect_container(name).await?;
        Ok(map_docker_instance(details))
    }

    async fn logs(&self, name: &str, options: &ServiceLogsOptions) -> Result<ServiceLogs> {
        let logs = self
            .docker
            .container_logs(
                name,
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
    fungi_home: PathBuf,
    allowed_host_paths: Arc<Mutex<Vec<PathBuf>>>,
    services: Arc<Mutex<HashMap<String, WasmtimeServiceState>>>,
}

pub(crate) struct WasmtimeServiceState {
    pub(crate) manifest: ServiceManifest,
    pub(crate) source_display: String,
    pub(crate) staged_component_path: PathBuf,
    pub(crate) service_dir: PathBuf,
    pub(crate) log_file_path: PathBuf,
    pub(crate) child: Option<Child>,
    pub(crate) last_exit_code: Option<i32>,
}

fn remove_dir_all_with_retry(path: &Path) -> std::io::Result<()> {
    let attempts = if cfg!(windows) { 10 } else { 1 };
    let mut last_error = None;

    for attempt in 1..=attempts {
        match fs::remove_dir_all(path) {
            Ok(()) => return Ok(()),
            Err(error)
                if attempt < attempts && matches!(error.raw_os_error(), Some(32) | Some(5)) =>
            {
                last_error = Some(error);
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(error) => return Err(error),
        }
    }

    Err(last_error.unwrap_or_else(|| std::io::Error::other("remove_dir_all failed")))
}

impl WasmtimeRuntimeProvider {
    pub fn new(
        runtime_root: PathBuf,
        launcher_path: PathBuf,
        fungi_home: PathBuf,
        allowed_host_paths: Vec<PathBuf>,
    ) -> Self {
        let allowed_host_paths = with_default_service_data_root(&fungi_home, allowed_host_paths);
        Self {
            runtime_root,
            launcher_path,
            fungi_home,
            allowed_host_paths: Arc::new(Mutex::new(allowed_host_paths)),
            services: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn update_allowed_host_paths(&self, allowed_host_paths: Vec<PathBuf>) {
        *self.allowed_host_paths.lock() =
            with_default_service_data_root(&self.fungi_home, allowed_host_paths);
    }

    pub fn has_service(&self, handle: &str) -> bool {
        self.services.lock().contains_key(handle)
    }

    pub(crate) async fn restore(&self, manifest: &ServiceManifest) -> Result<()> {
        let allowed_host_paths = self.allowed_host_paths.lock().clone();
        let state =
            build_wasmtime_state(&self.runtime_root, &allowed_host_paths, manifest, false).await?;
        let mut services = self.services.lock();
        services.entry(manifest.name.clone()).or_insert(state);
        Ok(())
    }
}

fn with_default_service_data_root(fungi_home: &Path, mut paths: Vec<PathBuf>) -> Vec<PathBuf> {
    paths.push(fungi_home.join("data"));
    paths.sort();
    paths.dedup();
    paths
}

#[async_trait]
impl RuntimeProvider for WasmtimeRuntimeProvider {
    fn runtime_kind(&self) -> RuntimeKind {
        RuntimeKind::Wasmtime
    }

    async fn pull(&self, manifest: &ServiceManifest) -> Result<ServiceInstance> {
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

        let mut command = build_wasmtime_command(&self.launcher_path, &self.fungi_home, state)?;
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
            remove_dir_all_with_retry(&service_dir).with_context(|| {
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
