use std::{env, path::{Path, PathBuf}, sync::Arc};

use anyhow::{Result, bail};
use fungi_config::docker::Docker as DockerConfig;
use fungi_docker_agent::{
    AgentPolicy, ContainerDetails, ContainerLogs, ContainerSpec, DockerAgent, LogsOptions,
    PortRule,
};

#[derive(Clone)]
pub struct DockerControl {
    agent: Arc<DockerAgent>,
}

impl DockerControl {
    pub fn from_config(config: &DockerConfig) -> Result<Option<Self>> {
        if !config.enabled {
            return Ok(None);
        }

        let socket_path = resolve_socket_path(config.socket_path.as_deref())?;
        let allowed_ports = config
            .allowed_ports
            .iter()
            .copied()
            .map(PortRule::Single)
            .chain(config.allowed_port_ranges.iter().map(|range| PortRule::Range {
                start: range.start,
                end: range.end,
            }))
            .collect();

        let policy = AgentPolicy {
            socket_path,
            managed_label_key: config.managed_label_key.clone(),
            managed_label_value: config.managed_label_value.clone(),
            allowed_host_paths: config.allowed_host_paths.clone(),
            allowed_ports,
        };

        Ok(Some(Self {
            agent: Arc::new(DockerAgent::new(policy)),
        }))
    }

    pub async fn create_container(&self, spec: &ContainerSpec) -> Result<ContainerDetails> {
        Ok(self.agent.create_container(spec).await?)
    }

    pub async fn start_container(&self, id_or_name: &str) -> Result<()> {
        Ok(self.agent.start_container(id_or_name).await?)
    }

    pub async fn stop_container(&self, id_or_name: &str) -> Result<()> {
        Ok(self.agent.stop_container(id_or_name).await?)
    }

    pub async fn remove_container(&self, id_or_name: &str) -> Result<()> {
        Ok(self.agent.remove_container(id_or_name).await?)
    }

    pub async fn inspect_container(&self, id_or_name: &str) -> Result<ContainerDetails> {
        Ok(self.agent.inspect_container(id_or_name).await?)
    }

    pub async fn container_logs(
        &self,
        id_or_name: &str,
        options: &LogsOptions,
    ) -> Result<ContainerLogs> {
        Ok(self.agent.container_logs(id_or_name, options).await?)
    }
}

fn resolve_socket_path(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        return Ok(path.to_path_buf());
    }

    if let Ok(host) = env::var("DOCKER_HOST")
        && let Some(path) = host.strip_prefix("unix://")
    {
        return Ok(PathBuf::from(path));
    }

    let home_socket = env::var("HOME")
        .ok()
        .map(PathBuf::from)
        .map(|home| home.join(".docker/run/docker.sock"));

    for candidate in [home_socket, Some(PathBuf::from("/var/run/docker.sock"))]
        .into_iter()
        .flatten()
    {
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    bail!("Docker is enabled but no docker socket could be detected")
}

#[cfg(test)]
mod tests {
    use super::*;
    use fungi_config::docker::{AllowedPortRange, Docker};

    #[test]
    fn disabled_docker_returns_no_control() {
        assert!(DockerControl::from_config(&Docker::default()).unwrap().is_none());
    }

    #[test]
    fn explicit_socket_path_creates_control() {
        let config = Docker {
            enabled: true,
            socket_path: Some(PathBuf::from("/tmp/docker.sock")),
            managed_label_key: "managed_by".into(),
            managed_label_value: "fungi".into(),
            allowed_host_paths: vec![PathBuf::from("/tmp/fungi")],
            allowed_ports: vec![8080],
            allowed_port_ranges: vec![AllowedPortRange {
                start: 20000,
                end: 20100,
            }],
        };

        assert!(DockerControl::from_config(&config).unwrap().is_some());
    }
}