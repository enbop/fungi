use anyhow::{Result, bail};
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

        self.allowed_host_paths = default_allowed_host_paths(fungi_dir);
    }

    pub fn validate_allowed_host_path(path: &Path, fungi_dir: &Path) -> Result<PathBuf> {
        let normalized = normalize_absolute_path(path)?;
        if is_sensitive_fungi_path(&normalized, fungi_dir)? {
            bail!(
                "refusing to allow fungi home secrets path: {}. Use {}/sandboxes or a directory outside fungi home",
                normalized.display(),
                normalize_absolute_path(fungi_dir.join("sandboxes").as_path())?.display()
            );
        }
        Ok(normalized)
    }
}

fn default_allowed_host_paths(fungi_dir: &Path) -> Vec<PathBuf> {
    vec![fungi_dir.join("sandboxes")]
}

fn is_sensitive_fungi_path(path: &Path, fungi_dir: &Path) -> Result<bool> {
    let fungi_home = normalize_absolute_path(fungi_dir)?;
    let sandboxes_root = normalize_absolute_path(&fungi_home.join("sandboxes"))?;

    Ok(path.starts_with(&fungi_home) && !path.starts_with(&sandboxes_root))
}

fn normalize_absolute_path(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute() {
        bail!("host path must be absolute: {}", path.display());
    }

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::RootDir => normalized.push(component.as_os_str()),
            std::path::Component::Normal(part) => normalized.push(part),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::Prefix(_) => normalized.push(component.as_os_str()),
        }
    }
    Ok(normalized)
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

    fn test_fungi_home() -> PathBuf {
        std::env::temp_dir().join("fungi-home")
    }

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

    #[test]
    fn runtime_defaults_only_allow_sandboxes_subdir() {
        let fungi_home = test_fungi_home();
        let mut runtime = Runtime::default();

        runtime.apply_default_allowed_host_paths(&fungi_home);

        assert_eq!(
            runtime.allowed_host_paths,
            vec![fungi_home.join("sandboxes")]
        );
    }

    #[test]
    fn runtime_defaults_preserve_explicit_allowed_host_paths() {
        let fungi_home = test_fungi_home();
        let mut runtime = Runtime {
            allowed_host_paths: vec![
                fungi_home.clone(),
                fungi_home.join("sandboxes"),
                fungi_home.join("sandboxes/demo"),
            ],
            ..Runtime::default()
        };

        runtime.apply_default_allowed_host_paths(&fungi_home);

        assert_eq!(
            runtime.allowed_host_paths,
            vec![
                fungi_home,
                test_fungi_home().join("sandboxes"),
                test_fungi_home().join("sandboxes/demo")
            ]
        );
    }

    #[test]
    fn validate_allowed_host_path_rejects_fungi_home_root() {
        let fungi_home = test_fungi_home();
        let error = Runtime::validate_allowed_host_path(&fungi_home, &fungi_home).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("refusing to allow fungi home secrets path")
        );
    }
}
