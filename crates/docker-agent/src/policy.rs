use crate::{DockerAgentError, Result, spec::ContainerSpec};
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone)]
pub struct AgentPolicy {
    pub socket_path: PathBuf,
    pub managed_label_key: String,
    pub managed_label_value: String,
    pub allowed_host_paths: Vec<PathBuf>,
    pub allowed_ports: Vec<PortRule>,
}

impl AgentPolicy {
    pub fn validate_create_spec(&self, spec: &ContainerSpec) -> Result<()> {
        if spec.image.trim().is_empty() {
            return Err(DockerAgentError::InvalidSpec(
                "image must not be empty".into(),
            ));
        }

        if let Some(name) = &spec.name
            && name.trim().is_empty()
        {
            return Err(DockerAgentError::InvalidSpec(
                "name must not be empty".into(),
            ));
        }

        for mount in &spec.mounts {
            if mount.container_path.trim().is_empty() || !mount.container_path.starts_with('/') {
                return Err(DockerAgentError::InvalidSpec(format!(
                    "container mount path must be absolute: {}",
                    mount.container_path
                )));
            }

            let host_path = normalize_absolute_path(&mount.host_path)?;
            let allowed = self
                .allowed_host_paths
                .iter()
                .map(|path| normalize_absolute_path(path))
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .any(|allowed_root| host_path.starts_with(&allowed_root));
            if !allowed {
                return Err(DockerAgentError::PolicyDenied(format!(
                    "host path is outside allowed roots: {}",
                    mount.host_path.display()
                )));
            }
        }

        for port in &spec.ports {
            if port.host_port == 0 || port.container_port == 0 {
                return Err(DockerAgentError::InvalidSpec(
                    "container and host ports must be greater than 0".into(),
                ));
            }

            if !self
                .allowed_ports
                .iter()
                .any(|rule| rule.allows(port.host_port))
            {
                return Err(DockerAgentError::PolicyDenied(format!(
                    "host port is not allowed: {}",
                    port.host_port
                )));
            }
        }

        Ok(())
    }

    pub fn managed_label(&self) -> (&str, &str) {
        (&self.managed_label_key, &self.managed_label_value)
    }
}

#[derive(Debug, Clone)]
pub enum PortRule {
    Single(u16),
    Range { start: u16, end: u16 },
}

impl PortRule {
    pub fn allows(&self, port: u16) -> bool {
        match self {
            Self::Single(value) => *value == port,
            Self::Range { start, end } => (*start..=*end).contains(&port),
        }
    }
}

fn normalize_absolute_path(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute() {
        return Err(DockerAgentError::InvalidSpec(format!(
            "host path must be absolute: {}",
            path.display()
        )));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BindMount, ContainerSpec, PortBinding};
    use std::collections::BTreeMap;

    fn allowed_root() -> PathBuf {
        std::env::temp_dir().join("fungi-policy-allowed")
    }

    fn disallowed_root() -> PathBuf {
        std::env::temp_dir().join("fungi-policy-denied")
    }

    fn sample_policy() -> AgentPolicy {
        AgentPolicy {
            socket_path: std::env::temp_dir().join("docker.sock"),
            managed_label_key: "managed_by".into(),
            managed_label_value: "fungi".into(),
            allowed_host_paths: vec![
                allowed_root(),
                std::env::current_dir().unwrap().join("test-data"),
            ],
            allowed_ports: vec![
                PortRule::Single(8080),
                PortRule::Range {
                    start: 20000,
                    end: 20100,
                },
            ],
        }
    }

    #[test]
    fn accepts_allowed_paths_and_ports() {
        let policy = sample_policy();
        let spec = ContainerSpec {
            image: "filebrowser/filebrowser:latest".into(),
            mounts: vec![BindMount {
                host_path: allowed_root().join("data"),
                container_path: "/data".into(),
            }],
            ports: vec![PortBinding {
                host_port: 20010,
                container_port: 80,
                protocol: Default::default(),
            }],
            env: BTreeMap::new(),
            ..Default::default()
        };

        assert!(policy.validate_create_spec(&spec).is_ok());
    }

    #[test]
    fn rejects_path_outside_whitelist() {
        let policy = sample_policy();
        let spec = ContainerSpec {
            image: "img".into(),
            mounts: vec![BindMount {
                host_path: disallowed_root(),
                container_path: "/data".into(),
            }],
            ..Default::default()
        };

        assert!(matches!(
            policy.validate_create_spec(&spec),
            Err(DockerAgentError::PolicyDenied(_))
        ));
    }

    #[test]
    fn rejects_port_outside_whitelist() {
        let policy = sample_policy();
        let spec = ContainerSpec {
            image: "img".into(),
            ports: vec![PortBinding {
                host_port: 9999,
                container_port: 80,
                protocol: Default::default(),
            }],
            ..Default::default()
        };

        assert!(matches!(
            policy.validate_create_spec(&spec),
            Err(DockerAgentError::PolicyDenied(_))
        ));
    }
}
