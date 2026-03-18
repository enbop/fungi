use std::{
    env,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::Result;
use fungi_config::runtime::Runtime as RuntimeConfig;
use fungi_docker_agent::{
    AgentPolicy, ContainerDetails, ContainerLogs, ContainerSpec, DockerAgent, LogsOptions, PortRule,
};
use parking_lot::Mutex;

const MANAGED_LABEL_KEY: &str = "managed_by";
const MANAGED_LABEL_VALUE: &str = "fungi";

#[derive(Clone)]
pub struct DockerControl {
    policy: Arc<Mutex<AgentPolicy>>,
}

impl DockerControl {
    pub fn from_config(config: &RuntimeConfig) -> Result<Option<Self>> {
        if config.disable_docker {
            return Ok(None);
        }

        let Some(socket_path) = detect_socket_path(config) else {
            log::info!(
                "Docker runtime unavailable on this host; continuing without Docker support"
            );
            return Ok(None);
        };

        Ok(Some(Self {
            policy: Arc::new(Mutex::new(build_agent_policy(config, socket_path))),
        }))
    }

    pub fn update_runtime_config(&self, config: &RuntimeConfig) -> Result<()> {
        let socket_path = self.policy.lock().socket_path.clone();
        *self.policy.lock() = build_agent_policy(config, socket_path);
        Ok(())
    }

    fn agent(&self) -> DockerAgent {
        DockerAgent::new(self.policy.lock().clone())
    }

    pub async fn create_container(&self, spec: &ContainerSpec) -> Result<ContainerDetails> {
        Ok(self.agent().create_container(spec).await?)
    }

    pub async fn start_container(&self, id_or_name: &str) -> Result<()> {
        Ok(self.agent().start_container(id_or_name).await?)
    }

    pub async fn stop_container(&self, id_or_name: &str) -> Result<()> {
        Ok(self.agent().stop_container(id_or_name).await?)
    }

    pub async fn remove_container(&self, id_or_name: &str) -> Result<()> {
        Ok(self.agent().remove_container(id_or_name).await?)
    }

    pub async fn inspect_container(&self, id_or_name: &str) -> Result<ContainerDetails> {
        Ok(self.agent().inspect_container(id_or_name).await?)
    }

    pub async fn container_logs(
        &self,
        id_or_name: &str,
        options: &LogsOptions,
    ) -> Result<ContainerLogs> {
        Ok(self.agent().container_logs(id_or_name, options).await?)
    }
}

pub fn detect_socket_path(config: &RuntimeConfig) -> Option<PathBuf> {
    resolve_socket_path(config.docker_socket_path.as_deref())
}

fn build_agent_policy(config: &RuntimeConfig, socket_path: PathBuf) -> AgentPolicy {
    let allowed_ports = config
        .allowed_ports
        .iter()
        .copied()
        .map(PortRule::Single)
        .chain(
            config
                .allowed_port_ranges
                .iter()
                .map(|range| PortRule::Range {
                    start: range.start,
                    end: range.end,
                }),
        )
        .collect();

    AgentPolicy {
        socket_path,
        managed_label_key: MANAGED_LABEL_KEY.into(),
        managed_label_value: MANAGED_LABEL_VALUE.into(),
        allowed_host_paths: config.allowed_host_paths.clone(),
        allowed_ports,
    }
}

fn resolve_socket_path(explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = explicit {
        return docker_endpoint_available(path).then(|| path.to_path_buf());
    }

    if let Ok(host) = env::var("DOCKER_HOST")
        && let Some(path) = host.strip_prefix("unix://")
    {
        let candidate = PathBuf::from(path);
        if docker_endpoint_available(&candidate) {
            return Some(candidate);
        }
    }

    #[cfg(windows)]
    if let Some(candidate) = docker_host_named_pipe_path(&host)
        && docker_endpoint_available(&candidate)
    {
        return Some(candidate);
    }

    #[cfg(unix)]
    {
        let home_socket = env::var("HOME")
            .ok()
            .map(PathBuf::from)
            .map(|home| home.join(".docker/run/docker.sock"));

        [home_socket, Some(PathBuf::from("/var/run/docker.sock"))]
            .into_iter()
            .flatten()
            .find(|candidate| docker_endpoint_available(candidate))
    }

    #[cfg(windows)]
    {
        let candidate = PathBuf::from(r"\\.\pipe\docker_engine");
        docker_endpoint_available(&candidate).then_some(candidate)
    }
}

#[cfg(unix)]
fn docker_endpoint_available(path: &Path) -> bool {
    path.exists()
}

#[cfg(windows)]
fn docker_endpoint_available(path: &Path) -> bool {
    if path.exists() {
        return true;
    }

    if !is_named_pipe_path(path) {
        return false;
    }

    tokio::net::windows::named_pipe::ClientOptions::new()
        .open(path)
        .is_ok()
}

#[cfg(windows)]
fn docker_host_named_pipe_path(host: &str) -> Option<PathBuf> {
    let raw = host.strip_prefix("npipe://")?;
    Some(normalize_named_pipe_path(raw))
}

#[cfg(windows)]
fn normalize_named_pipe_path(raw: &str) -> PathBuf {
    let normalized = raw.trim_start_matches('/').replace('/', "\\");
    if normalized.starts_with("\\\\.\\pipe\\") {
        return PathBuf::from(normalized);
    }
    if normalized.starts_with(".\\pipe\\") {
        return PathBuf::from(format!("\\\\{}", normalized));
    }
    PathBuf::from(normalized)
}

#[cfg(windows)]
fn is_named_pipe_path(path: &Path) -> bool {
    let value = path.as_os_str().to_string_lossy();
    value.starts_with("\\\\.\\pipe\\")
        || value.starts_with(".\\pipe\\")
        || value.starts_with("//./pipe/")
        || value.starts_with("npipe://")
}

#[cfg(test)]
mod tests {
    use super::*;
    use fungi_config::runtime::{AllowedPortRange, Runtime};
    use tempfile::TempDir;

    #[test]
    fn disabled_docker_returns_no_control() {
        let config = Runtime {
            disable_docker: true,
            ..Runtime::default()
        };
        assert!(DockerControl::from_config(&config).unwrap().is_none());
    }

    #[test]
    fn explicit_socket_path_creates_control() {
        let temp_dir = TempDir::new().unwrap();
        let socket_path = temp_dir.path().join("fungi-docker-test.sock");
        std::fs::File::create(&socket_path).unwrap();
        let config = Runtime {
            docker_socket_path: Some(socket_path.clone()),
            allowed_host_paths: vec![temp_dir.path().join("fungi")],
            allowed_ports: vec![8080],
            allowed_port_ranges: vec![AllowedPortRange {
                start: 20000,
                end: 20100,
            }],
            ..Runtime::default()
        };

        assert!(DockerControl::from_config(&config).unwrap().is_some());
    }
}
