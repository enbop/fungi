use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
};

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use fungi_config::paths::FungiPaths;
use fungi_docker_agent::LogsOptions;
use parking_lot::Mutex;
use tokio::process::Child;

use crate::controls::DockerControl;

use super::{
    helpers::{
        build_wasmtime_command, build_wasmtime_state, docker_spec_from_manifest_with_name,
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
    true
}

#[derive(Clone)]
pub struct DockerRuntimeProvider {
    docker: DockerControl,
}

impl DockerRuntimeProvider {
    pub fn new(docker: DockerControl) -> Self {
        Self { docker }
    }

    pub(crate) async fn pull_with_container_name(
        &self,
        manifest: &ServiceManifest,
        container_name: &str,
    ) -> Result<ServiceInstance> {
        ensure_manifest_mount_dirs(manifest)?;
        let spec = docker_spec_from_manifest_with_name(manifest, container_name)?;
        let details = self.docker.create_container(&spec).await?;
        Ok(map_docker_instance(details))
    }
}

#[async_trait]
impl RuntimeProvider for DockerRuntimeProvider {
    fn runtime_kind(&self) -> RuntimeKind {
        RuntimeKind::Docker
    }

    async fn pull(&self, manifest: &ServiceManifest) -> Result<ServiceInstance> {
        self.pull_with_container_name(manifest, &manifest.name)
            .await
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
    pub(crate) runtime_dir: PathBuf,
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
    pub(crate) fn logs_text_bounded(
        &self,
        handle: &str,
        tail: usize,
        max_bytes: usize,
    ) -> Result<BoundedLogText> {
        let log_file_path = {
            let services = self.services.lock();
            services
                .get(handle)
                .ok_or_else(|| anyhow::anyhow!("wasmtime service not found: {handle}"))?
                .log_file_path
                .clone()
        };

        let mut file = match fs::File::open(&log_file_path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(BoundedLogText {
                    text: String::new(),
                    truncated: false,
                });
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("Failed to read log file: {}", log_file_path.display())
                });
            }
        };
        read_tail_text_bounded(&mut file, tail, max_bytes)
            .with_context(|| format!("Failed to read log file: {}", log_file_path.display()))
    }

    pub fn new(
        runtime_root: PathBuf,
        launcher_path: PathBuf,
        fungi_home: PathBuf,
        allowed_host_paths: Vec<PathBuf>,
    ) -> Self {
        let allowed_host_paths = with_default_mount_roots(&fungi_home, allowed_host_paths);
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
            with_default_mount_roots(&self.fungi_home, allowed_host_paths);
    }

    pub fn has_service(&self, handle: &str) -> bool {
        self.services.lock().contains_key(handle)
    }

    pub(crate) async fn restore(
        &self,
        manifest: &ServiceManifest,
        local_service_id: &str,
    ) -> Result<()> {
        let allowed_host_paths = self.allowed_host_paths.lock().clone();
        let state = build_wasmtime_state(
            &self.runtime_root,
            &self.service_artifacts_dir(local_service_id),
            &allowed_host_paths,
            manifest,
            false,
        )
        .await?;
        let mut services = self.services.lock();
        services.entry(manifest.name.clone()).or_insert(state);
        Ok(())
    }

    pub(crate) async fn pull_with_local_service_id(
        &self,
        manifest: &ServiceManifest,
        local_service_id: &str,
    ) -> Result<ServiceInstance> {
        self.apply_with_local_service_id(manifest, local_service_id, false)
            .await
    }

    pub(crate) async fn replace_with_local_service_id(
        &self,
        manifest: &ServiceManifest,
        local_service_id: &str,
    ) -> Result<ServiceInstance> {
        self.apply_with_local_service_id(manifest, local_service_id, true)
            .await
    }

    async fn apply_with_local_service_id(
        &self,
        manifest: &ServiceManifest,
        local_service_id: &str,
        replace_existing: bool,
    ) -> Result<ServiceInstance> {
        if replace_existing {
            self.remove_with_local_service_id(&manifest.name, local_service_id)
                .await?;
        }

        let allowed_host_paths = self.allowed_host_paths.lock().clone();
        let state = build_wasmtime_state(
            &self.runtime_root,
            &self.service_artifacts_dir(local_service_id),
            &allowed_host_paths,
            manifest,
            true,
        )
        .await?;

        {
            let mut services = self.services.lock();
            if services.contains_key(&manifest.name) {
                bail!("service already exists: {}", manifest.name);
            }
            services.insert(manifest.name.clone(), state);
        }

        self.inspect(&manifest.name).await
    }

    pub(crate) async fn remove_with_local_service_id(
        &self,
        handle: &str,
        local_service_id: &str,
    ) -> Result<()> {
        self.remove_artifacts_and_runtime(handle, Some(local_service_id))
            .await
    }

    fn service_artifacts_dir(&self, local_service_id: &str) -> PathBuf {
        FungiPaths::from_fungi_home(&self.fungi_home).service_artifacts_dir(local_service_id)
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Read, Seek, SeekFrom};

    use super::*;

    struct CountingReader {
        inner: Cursor<Vec<u8>>,
        bytes_read: usize,
    }

    impl CountingReader {
        fn new(bytes: Vec<u8>) -> Self {
            Self {
                inner: Cursor::new(bytes),
                bytes_read: 0,
            }
        }
    }

    impl Read for CountingReader {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            let read = self.inner.read(buffer)?;
            self.bytes_read += read;
            Ok(read)
        }
    }

    impl Seek for CountingReader {
        fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
            self.inner.seek(position)
        }
    }

    #[test]
    fn bounded_tail_reads_only_the_file_suffix() {
        let mut bytes = vec![b'x'; 2 * 1024 * 1024];
        bytes.extend_from_slice(b"\nkeep one\nkeep two\n");
        let mut reader = CountingReader::new(bytes);

        let logs = read_tail_text_bounded(&mut reader, 2, 64).unwrap();

        assert_eq!(logs.text, "keep one\nkeep two\n");
        assert!(!logs.truncated);
        assert_eq!(reader.bytes_read, 65);
    }

    #[test]
    fn bounded_tail_limits_a_single_oversized_line() {
        let mut reader = CountingReader::new(vec![b'x'; 1024]);

        let logs = read_tail_text_bounded(&mut reader, 1, 32).unwrap();

        assert_eq!(logs.text, "x".repeat(32));
        assert!(logs.truncated);
        assert_eq!(reader.bytes_read, 33);
    }

    #[test]
    fn bounded_tail_preserves_utf8_and_trailing_newline_semantics() {
        let mut bytes = vec![b'x'; 1024];
        bytes.extend_from_slice("\n前一行\n最后一行\n".as_bytes());
        let mut reader = CountingReader::new(bytes);

        let logs = read_tail_text_bounded(&mut reader, 1, 64).unwrap();

        assert_eq!(logs.text, "最后一行\n");
        assert!(!logs.truncated);
        assert!(logs.text.is_char_boundary(logs.text.len()));
    }

    #[test]
    fn bounded_tail_handles_invalid_utf8_without_exceeding_budget() {
        let mut bytes = vec![b'x'; 128];
        bytes.extend_from_slice(b"\n\xfftail\n");
        let mut reader = CountingReader::new(bytes);

        let logs = read_tail_text_bounded(&mut reader, 1, 16).unwrap();

        assert_eq!(logs.text, "�tail\n");
        assert!(logs.text.len() <= 16);
        assert!(!logs.truncated);
    }
}

pub(crate) fn limit_text_bytes_from_end(text: String, max_bytes: usize) -> BoundedLogText {
    if text.len() <= max_bytes {
        return BoundedLogText {
            text,
            truncated: false,
        };
    }

    if max_bytes == 0 {
        return BoundedLogText {
            text: String::new(),
            truncated: !text.is_empty(),
        };
    }

    let mut start = text.len() - max_bytes;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    BoundedLogText {
        text: text[start..].to_string(),
        truncated: true,
    }
}

fn read_tail_text_bounded<R: Read + Seek>(
    reader: &mut R,
    tail: usize,
    max_bytes: usize,
) -> Result<BoundedLogText> {
    let file_len = reader.seek(SeekFrom::End(0))?;
    if file_len == 0 || tail == 0 || max_bytes == 0 {
        return Ok(BoundedLogText {
            text: String::new(),
            truncated: file_len > 0,
        });
    }

    let read_budget = max_bytes.saturating_add(1);
    let read_len = usize::try_from(file_len.min(read_budget as u64))?;
    let read_start = file_len - read_len as u64;
    reader.seek(SeekFrom::Start(read_start))?;

    let mut raw = vec![0_u8; read_len];
    reader.read_exact(&mut raw)?;

    let scan_end = raw.len() - usize::from(raw.ends_with(b"\n"));
    let mut remaining_lines = tail;
    let mut selected_start = raw.len().saturating_sub(max_bytes);
    for index in (0..scan_end).rev() {
        if raw[index] != b'\n' {
            continue;
        }
        remaining_lines -= 1;
        if remaining_lines == 0 {
            selected_start = index + 1;
            break;
        }
    }

    let source_exceeded_budget = file_len > max_bytes as u64;
    let incomplete_requested_lines = source_exceeded_budget && remaining_lines > 0;
    let text = String::from_utf8_lossy(&raw[selected_start..]).into_owned();
    let mut bounded = limit_text_bytes_from_end(text, max_bytes);
    bounded.truncated |= incomplete_requested_lines;
    Ok(bounded)
}

fn with_default_mount_roots(fungi_home: &Path, mut paths: Vec<PathBuf>) -> Vec<PathBuf> {
    paths.extend(fungi_config::runtime::Runtime::default_allowed_host_paths(
        fungi_home,
    ));
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
        self.pull_with_local_service_id(manifest, &manifest.name)
            .await
    }

    async fn start(&self, handle: &str) -> Result<()> {
        let mut services = self.services.lock();
        let state = services
            .get_mut(handle)
            .ok_or_else(|| anyhow::anyhow!("wasmtime service not found: {handle}"))?;

        refresh_child_state(state)?;
        if state.child.is_some() {
            return Ok(());
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
        self.remove_artifacts_and_runtime(handle, None).await
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

impl WasmtimeRuntimeProvider {
    async fn remove_artifacts_and_runtime(
        &self,
        handle: &str,
        local_service_id: Option<&str>,
    ) -> Result<()> {
        self.stop(handle).await.ok();

        let state = {
            let mut services = self.services.lock();
            services.remove(handle)
        };

        let (service_dir, runtime_dir) = state
            .map(|state| (state.service_dir, state.runtime_dir))
            .unwrap_or_else(|| {
                let service_dir = local_service_id
                    .map(|local_service_id| self.service_artifacts_dir(local_service_id))
                    .unwrap_or_else(|| self.service_artifacts_dir(handle));
                let runtime_dir = self.runtime_root.join("wasmtime").join(handle);
                (service_dir, runtime_dir)
            });

        if service_dir.exists() {
            remove_dir_all_with_retry(&service_dir).with_context(|| {
                format!(
                    "Failed to remove service artifacts directory: {}",
                    service_dir.display()
                )
            })?;
        }
        if runtime_dir.exists() {
            remove_dir_all_with_retry(&runtime_dir).with_context(|| {
                format!(
                    "Failed to remove runtime directory: {}",
                    runtime_dir.display()
                )
            })?;
        }
        Ok(())
    }
}
