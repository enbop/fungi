use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Docker {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub socket_path: Option<PathBuf>,
    #[serde(default = "default_managed_label_key")]
    pub managed_label_key: String,
    #[serde(default = "default_managed_label_value")]
    pub managed_label_value: String,
    #[serde(default)]
    pub allowed_host_paths: Vec<PathBuf>,
    #[serde(default)]
    pub allowed_ports: Vec<u16>,
    #[serde(default)]
    pub allowed_port_ranges: Vec<AllowedPortRange>,
}

impl Default for Docker {
    fn default() -> Self {
        Self {
            enabled: false,
            socket_path: None,
            managed_label_key: default_managed_label_key(),
            managed_label_value: default_managed_label_value(),
            allowed_host_paths: Vec::new(),
            allowed_ports: Vec::new(),
            allowed_port_ranges: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AllowedPortRange {
    pub start: u16,
    pub end: u16,
}

fn default_managed_label_key() -> String {
    "managed_by".into()
}

fn default_managed_label_value() -> String {
    "fungi".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_docker_policy_is_denied_by_default() {
        let docker = Docker::default();
        assert!(!docker.enabled);
        assert!(docker.allowed_host_paths.is_empty());
        assert!(docker.allowed_ports.is_empty());
        assert!(docker.allowed_port_ranges.is_empty());
        assert_eq!(docker.managed_label_key, "managed_by");
        assert_eq!(docker.managed_label_value, "fungi");
    }
}