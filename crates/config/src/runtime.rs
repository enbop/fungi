use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Runtime {
    #[serde(default)]
    pub disable_docker: bool,
    #[serde(default)]
    pub disable_wasmtime: bool,
    #[serde(default)]
    pub docker_socket_path: Option<PathBuf>,
    #[serde(default)]
    pub allowed_host_paths: Vec<PathBuf>,
    #[serde(default)]
    pub allowed_ports: Vec<u16>,
    #[serde(default = "default_allowed_port_ranges")]
    pub allowed_port_ranges: Vec<AllowedPortRange>,
}

impl Default for Runtime {
    fn default() -> Self {
        Self {
            disable_docker: false,
            disable_wasmtime: false,
            docker_socket_path: None,
            allowed_host_paths: Vec::new(),
            allowed_ports: Vec::new(),
            allowed_port_ranges: default_allowed_port_ranges(),
        }
    }
}

impl Runtime {
    pub fn docker_enabled(&self) -> bool {
        !self.disable_docker
    }

    pub fn wasmtime_enabled(&self) -> bool {
        !self.disable_wasmtime
    }

    pub fn apply_default_allowed_host_paths(&mut self, fungi_dir: &Path) {
        if !self.allowed_host_paths.is_empty() {
            return;
        }

        self.allowed_host_paths = vec![fungi_dir.to_path_buf(), fungi_dir.join("services")];
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct AllowedPortRange {
    pub start: u16,
    pub end: u16,
}

fn default_allowed_port_ranges() -> Vec<AllowedPortRange> {
    vec![AllowedPortRange {
        start: 18080,
        end: 18199,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_runtime_policy_is_enabled_with_safe_defaults() {
        let runtime = Runtime::default();
        assert!(runtime.docker_enabled());
        assert!(runtime.wasmtime_enabled());
        assert!(runtime.allowed_host_paths.is_empty());
        assert!(runtime.allowed_ports.is_empty());
        assert_eq!(
            runtime.allowed_port_ranges,
            vec![AllowedPortRange {
                start: 18080,
                end: 18199,
            }]
        );
    }
}