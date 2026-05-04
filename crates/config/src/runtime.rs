use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::paths::FungiPaths;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Runtime {
    #[serde(default)]
    pub disable_docker: bool,
    #[serde(default)]
    pub disable_wasmtime: bool,
    #[serde(default)]
    pub docker_socket_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_host_paths: Vec<PathBuf>,
}

impl Default for Runtime {
    fn default() -> Self {
        Self {
            disable_docker: false,
            disable_wasmtime: false,
            docker_socket_path: None,
            allowed_host_paths: Vec::new(),
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

    pub fn validate_allowed_host_path(path: &Path, fungi_dir: &Path) -> Result<PathBuf> {
        let normalized = normalize_absolute_path(path)?;
        if is_sensitive_fungi_path(&normalized, fungi_dir)? {
            bail!(
                "refusing to allow fungi home control-plane path: {}. Use {} or a directory outside fungi home",
                normalized.display(),
                normalize_absolute_path(&FungiPaths::from_fungi_home(fungi_dir).appdata_root())?
                    .display()
            );
        }
        Ok(normalized)
    }
}

fn is_sensitive_fungi_path(path: &Path, fungi_dir: &Path) -> Result<bool> {
    let fungi_home = normalize_absolute_path(fungi_dir)?;
    let appdata_root =
        normalize_absolute_path(&FungiPaths::from_fungi_home(&fungi_home).appdata_root())?;

    Ok(path.starts_with(&fungi_home) && !path.starts_with(&appdata_root))
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
    }

    #[test]
    fn runtime_preserves_explicit_allowed_host_paths() {
        let fungi_home = test_fungi_home();
        let runtime = Runtime {
            allowed_host_paths: vec![
                fungi_home.join("appdata"),
                fungi_home.join("appdata/services/demo"),
            ],
            ..Runtime::default()
        };

        assert_eq!(
            runtime.allowed_host_paths,
            vec![
                fungi_home.join("appdata"),
                fungi_home.join("appdata/services/demo")
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
                .contains("refusing to allow fungi home control-plane path")
        );
    }

    #[test]
    fn validate_allowed_host_path_allows_service_appdata_root() {
        let fungi_home = test_fungi_home();
        let path = FungiPaths::from_fungi_home(&fungi_home).appdata_root();

        assert_eq!(
            Runtime::validate_allowed_host_path(&path, &fungi_home).unwrap(),
            path
        );
    }
}
