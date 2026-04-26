use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};

use crate::tcp_tunneling::ForwardingRule;

const LOCAL_ACCESS_FILE: &str = "access/local_access.json";

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct LocalAccessConfig {
    #[serde(default)]
    pub rules: Vec<ForwardingRule>,

    #[serde(skip)]
    config_file: PathBuf,
}

impl LocalAccessConfig {
    pub fn apply_from_dir(fungi_dir: &Path) -> Result<Self> {
        let config_file = fungi_dir.join(LOCAL_ACCESS_FILE);
        if !config_file.exists() {
            Self::init_config_file(config_file.clone())?;
        }

        let raw = std::fs::read_to_string(&config_file).with_context(|| {
            format!(
                "failed to read local access config: {}",
                config_file.display()
            )
        })?;
        let mut config: Self = serde_json::from_str(&raw).with_context(|| {
            format!(
                "failed to parse local access config: {}",
                config_file.display()
            )
        })?;
        config.config_file = config_file;
        Ok(config)
    }

    pub fn init_config_file(config_file: PathBuf) -> Result<()> {
        if config_file.exists() {
            return Ok(());
        }
        if let Some(parent) = config_file.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create local access directory: {}",
                    parent.display()
                )
            })?;
        }
        let raw = serde_json::to_string_pretty(&Self::default())?;
        std::fs::write(&config_file, raw).with_context(|| {
            format!(
                "failed to write local access config: {}",
                config_file.display()
            )
        })?;
        Ok(())
    }

    pub fn save_to_file(&self) -> Result<()> {
        let raw = serde_json::to_string_pretty(self)?;
        std::fs::write(&self.config_file, raw).with_context(|| {
            format!(
                "failed to write local access config: {}",
                self.config_file.display()
            )
        })?;
        Ok(())
    }

    pub fn add_forwarding_rule(&self, rule: ForwardingRule) -> Result<Self> {
        if self
            .rules
            .iter()
            .any(|existing| forwarding_rule_matches(existing, &rule))
        {
            return Ok(self.clone());
        }

        let mut updated = self.clone();
        updated.rules.push(rule);
        updated.rules.sort_by(|left, right| {
            left.remote_peer_id
                .cmp(&right.remote_peer_id)
                .then(left.remote_service_id.cmp(&right.remote_service_id))
                .then(
                    left.remote_service_port_name
                        .cmp(&right.remote_service_port_name),
                )
                .then(left.local_port.cmp(&right.local_port))
        });
        updated.save_to_file()?;
        Ok(updated)
    }

    pub fn remove_forwarding_rule(&self, rule: &ForwardingRule) -> Result<Self> {
        let mut updated = self.clone();
        updated
            .rules
            .retain(|existing| !forwarding_rule_matches(existing, rule));
        updated.save_to_file()?;
        Ok(updated)
    }
}

fn forwarding_rule_matches(left: &ForwardingRule, right: &ForwardingRule) -> bool {
    left.local_host == right.local_host
        && left.local_port == right.local_port
        && left.remote_peer_id == right.remote_peer_id
        && left.remote_protocol == right.remote_protocol
        && left.remote_port == right.remote_port
        && left.remote_service_id == right.remote_service_id
        && left.remote_service_port_name == right.remote_service_port_name
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn stores_local_access_rules_outside_config_toml() {
        let dir = TempDir::new().unwrap();
        let config = LocalAccessConfig::apply_from_dir(dir.path()).unwrap();

        let updated = config.add_forwarding_rule(rule("svc")).unwrap();

        assert_eq!(updated.rules.len(), 1);
        assert!(dir.path().join("access").join("local_access.json").exists());
    }

    fn rule(service_id: &str) -> ForwardingRule {
        ForwardingRule {
            local_host: "127.0.0.1".to_string(),
            local_port: 2222,
            remote_peer_id: "peer".to_string(),
            remote_protocol: Some("/fungi/service/svc/main/0.2.0".to_string()),
            remote_port: 22,
            remote_service_id: Some(service_id.to_string()),
            remote_service_name: Some(service_id.to_string()),
            remote_service_port_name: Some("main".to_string()),
        }
    }
}
