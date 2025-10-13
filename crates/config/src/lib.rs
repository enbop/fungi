pub mod address_book;
pub mod file_transfer;
mod init;
mod libp2p;
mod rpc;
pub mod tcp_tunneling;

pub use init::init;

use anyhow::Result;
use libp2p::*;
use libp2p_identity::PeerId;
use serde::{Deserialize, Serialize};
use std::{
    net::IpAddr,
    path::{Path, PathBuf},
};
use tcp_tunneling::*;

use crate::file_transfer::{FileTransfer, FileTransferClient, FileTransferService};

pub const DEFAULT_CONFIG_FILE: &str = "config.toml";
pub const DEFAULT_FUNGI_DIR: &str = ".fungi";

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct FungiConfig {
    #[serde(default)]
    pub rpc: rpc::Rpc,
    #[serde(default)]
    pub network: Network,
    #[serde(default)]
    pub tcp_tunneling: TcpTunneling,
    #[serde(default)]
    pub file_transfer: FileTransfer,

    #[serde(default)]
    custom_hostname: Option<String>,

    #[serde(skip)]
    config_file: PathBuf,
}

impl FungiConfig {
    pub fn config_file_path(&self) -> &Path {
        &self.config_file
    }

    pub fn init_config_file(config_file: PathBuf) -> Result<()> {
        if config_file.exists() {
            return Ok(());
        }
        let config = FungiConfig::default();
        let toml_string = toml::to_string(&config)?;
        std::fs::write(&config_file, toml_string)?;
        Ok(())
    }

    pub fn try_read_from_dir(fungi_dir: &Path) -> Result<Self> {
        let config_file = fungi_dir.join(DEFAULT_CONFIG_FILE);
        if !config_file.exists() {
            return Err(anyhow::anyhow!(
                "Config file not found at {}",
                config_file.display()
            ));
        }
        let s = std::fs::read_to_string(&config_file)?;
        let mut cfg = Self::parse_toml(&s)?;
        cfg.config_file = config_file;
        Ok(cfg)
    }

    pub fn apply_from_dir(fungi_dir: &Path) -> Result<Self> {
        let config_file = fungi_dir.join(DEFAULT_CONFIG_FILE);
        if !config_file.exists() {
            Self::init_config_file(config_file.clone())?;
        }

        println!("Loading Fungi config from: {config_file:?}");
        let s = std::fs::read_to_string(&config_file)?;
        let mut cfg = Self::parse_toml(&s)?;
        cfg.config_file = config_file;
        Ok(cfg)
    }

    pub fn parse_toml(s: &str) -> Result<Self> {
        let config: Self = toml::from_str(s)?;
        Ok(config)
    }

    /// Save current config to file
    pub fn save_to_file(&self) -> Result<()> {
        let toml_string = toml::to_string_pretty(self)?;
        std::fs::write(&self.config_file, toml_string)?;
        Ok(())
    }

    /// Create a new config with updated field and save to file
    fn update_and_save<F>(&self, updater: F) -> Result<Self>
    where
        F: FnOnce(&mut Self),
    {
        let mut new_config = self.clone();
        updater(&mut new_config);
        new_config.save_to_file()?;
        Ok(new_config)
    }

    pub fn get_hostname(&self) -> Option<String> {
        self.custom_hostname
            .as_ref()
            .cloned()
            .or(fungi_util::get_hostname())
    }

    pub fn set_custom_hostname(&self, hostname: Option<String>) -> Result<Self> {
        self.update_and_save(|config| {
            config.custom_hostname = hostname;
        })
    }

    pub fn add_incoming_allowed_peer(&self, peer_id: &PeerId) -> Result<Self> {
        if self.network.incoming_allowed_peers.contains(peer_id) {
            return Ok(self.clone());
        }

        self.update_and_save(|config| {
            config.network.incoming_allowed_peers.push(*peer_id);
        })
    }

    pub fn remove_incoming_allowed_peer(&self, peer_id: &PeerId) -> Result<Self> {
        self.update_and_save(|config| {
            config
                .network
                .incoming_allowed_peers
                .retain(|p| p != peer_id);
        })
    }

    pub fn update_file_transfer_service(
        &self,
        enabled: bool,
        root_dir: PathBuf,
    ) -> Result<(Self, FileTransferService)> {
        let new_config = self.update_and_save(|config| {
            config.file_transfer.server.enabled = enabled;
            config.file_transfer.server.shared_root_dir = root_dir.clone();
        })?;

        Ok((new_config.clone(), new_config.file_transfer.server.clone()))
    }

    pub fn add_file_transfer_client(
        &self,
        enabled: bool,
        peer_id: PeerId,
        name: Option<String>,
    ) -> Result<Self> {
        if self
            .file_transfer
            .client
            .iter()
            .any(|c| c.peer_id == peer_id)
        {
            return Ok(self.clone());
        }

        self.update_and_save(|config| {
            let client = FileTransferClient {
                enabled,
                name,
                peer_id,
            };
            config.file_transfer.client.push(client);
        })
    }

    pub fn remove_file_transfer_client(&self, peer_id: &PeerId) -> Result<Self> {
        self.update_and_save(|config| {
            config
                .file_transfer
                .client
                .retain(|c| c.peer_id != *peer_id);
        })
    }

    pub fn enable_file_transfer_client(
        &self,
        peer_id: &PeerId,
        enabled: bool,
    ) -> Result<(Self, Option<FileTransferClient>)> {
        let client_exists = self
            .file_transfer
            .client
            .iter()
            .any(|c| c.peer_id == *peer_id);
        if !client_exists {
            return Ok((self.clone(), None));
        }

        let new_config = self.update_and_save(|config| {
            if let Some(client) = config
                .file_transfer
                .client
                .iter_mut()
                .find(|c| c.peer_id == *peer_id)
            {
                client.enabled = enabled;
            }
        })?;

        let updated_client = new_config
            .file_transfer
            .client
            .iter()
            .find(|c| c.peer_id == *peer_id)
            .cloned();

        Ok((new_config, updated_client))
    }

    pub fn update_ftp_proxy(&self, enabled: bool, host: IpAddr, port: u16) -> Result<Self> {
        self.update_and_save(|config| {
            config.file_transfer.proxy_ftp.enabled = enabled;
            config.file_transfer.proxy_ftp.host = host;
            config.file_transfer.proxy_ftp.port = port;
        })
    }

    pub fn update_webdav_proxy(&self, enabled: bool, host: IpAddr, port: u16) -> Result<Self> {
        self.update_and_save(|config| {
            config.file_transfer.proxy_webdav.enabled = enabled;
            config.file_transfer.proxy_webdav.host = host;
            config.file_transfer.proxy_webdav.port = port;
        })
    }

    /// Add TCP tunneling methods
    pub fn add_tcp_forwarding_rule(
        &self,
        rule: crate::tcp_tunneling::ForwardingRule,
    ) -> Result<Self> {
        // Check if rule already exists
        let rule_exists = self.tcp_tunneling.forwarding.rules.iter().any(|r| {
            r.local_host == rule.local_host
                && r.local_port == rule.local_port
                && r.remote_peer_id == rule.remote_peer_id
                && r.remote_port == rule.remote_port
        });

        if rule_exists {
            return Ok(self.clone());
        }

        self.update_and_save(|config| {
            config.tcp_tunneling.forwarding.rules.push(rule);
            if !config.tcp_tunneling.forwarding.enabled {
                config.tcp_tunneling.forwarding.enabled = true;
            }
        })
    }

    pub fn remove_tcp_forwarding_rule(
        &self,
        rule: &crate::tcp_tunneling::ForwardingRule,
    ) -> Result<Self> {
        self.update_and_save(|config| {
            config.tcp_tunneling.forwarding.rules.retain(|r| {
                !(r.local_host == rule.local_host
                    && r.local_port == rule.local_port
                    && r.remote_peer_id == rule.remote_peer_id
                    && r.remote_port == rule.remote_port)
            });
        })
    }

    pub fn add_tcp_listening_rule(
        &self,
        rule: crate::tcp_tunneling::ListeningRule,
    ) -> Result<Self> {
        // Check if rule already exists
        let rule_exists = self
            .tcp_tunneling
            .listening
            .rules
            .iter()
            .any(|r| r.host == rule.host && r.port == rule.port);

        if rule_exists {
            return Ok(self.clone());
        }

        self.update_and_save(|config| {
            config.tcp_tunneling.listening.rules.push(rule);
            if !config.tcp_tunneling.listening.enabled {
                config.tcp_tunneling.listening.enabled = true;
            }
        })
    }

    pub fn remove_tcp_listening_rule(
        &self,
        rule: &crate::tcp_tunneling::ListeningRule,
    ) -> Result<Self> {
        self.update_and_save(|config| {
            config
                .tcp_tunneling
                .listening
                .rules
                .retain(|r| !(r.host == rule.host && r.port == rule.port));
        })
    }
}

pub trait FungiDir {
    fn fungi_dir(&self) -> PathBuf;
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // return TempDir to keep lifetime
    fn create_temp_config() -> (FungiConfig, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join("fungi-test");
        if config_dir.exists() {
            std::fs::remove_dir_all(&config_dir).ok();
        }
        std::fs::create_dir_all(&config_dir).ok();
        (FungiConfig::apply_from_dir(&config_dir).unwrap(), temp_dir)
    }

    #[test]
    fn test_init_config_file() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join(DEFAULT_CONFIG_FILE);
        std::fs::remove_file(&config_path).ok();
        FungiConfig::init_config_file(config_path.clone()).unwrap();
        assert!(config_path.exists());
        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("[network]"));
        assert!(content.contains("[tcp_tunneling"));
        assert!(content.contains("[file_transfer"));
    }

    #[test]
    fn test_add_incoming_allowed_peer() {
        let (config, _path) = create_temp_config();
        let peer_id = PeerId::random();
        let updated_config = config.add_incoming_allowed_peer(&peer_id).unwrap();
        assert!(
            updated_config
                .network
                .incoming_allowed_peers
                .contains(&peer_id)
        );
        assert!(config.config_file.exists());
        let content = std::fs::read_to_string(&config.config_file).unwrap();
        println!("Config content: {content}");
        assert!(content.contains(&peer_id.to_string()));
    }

    #[test]
    fn test_remove_incoming_allowed_peer() {
        let (config, _temp_dir) = create_temp_config();

        // Add a peer first
        let peer_id = PeerId::random();
        let config_with_peer = config.add_incoming_allowed_peer(&peer_id).unwrap();
        assert!(
            config_with_peer
                .network
                .incoming_allowed_peers
                .contains(&peer_id)
        );

        // Remove it
        let final_config = config_with_peer
            .remove_incoming_allowed_peer(&peer_id)
            .unwrap();

        // Verify it's removed from memory
        assert!(
            !final_config
                .network
                .incoming_allowed_peers
                .contains(&peer_id)
        );

        // Verify changes were persisted
        let content = std::fs::read_to_string(&config.config_file).unwrap();
        assert!(!content.contains(&peer_id.to_string()));
    }

    #[test]
    fn test_update_file_transfer_service() {
        let (config, _temp_dir) = create_temp_config();
        let root_dir = PathBuf::from("/test/path");

        let (updated_config, service) = config
            .update_file_transfer_service(true, root_dir.clone())
            .unwrap();

        // Verify memory updates
        assert!(service.enabled);
        assert_eq!(service.shared_root_dir, root_dir);
        assert!(updated_config.file_transfer.server.enabled);
        assert_eq!(
            updated_config.file_transfer.server.shared_root_dir,
            root_dir
        );

        // Verify persisted changes
        let content = std::fs::read_to_string(&config.config_file).unwrap();
        println!("Config content: {content}");
        assert!(content.contains(&format!(
            "shared_root_dir = \"{}\"",
            root_dir.to_string_lossy()
        )));
        assert!(content.contains("enabled = true"));
    }

    #[test]
    fn test_add_file_transfer_client() {
        let (config, _temp_dir) = create_temp_config();
        let peer_id = PeerId::random();
        let client_name = "test-client";

        let updated_config = config
            .add_file_transfer_client(true, peer_id, Some(client_name.to_string()))
            .unwrap();

        // Verify memory updates
        let client = updated_config
            .file_transfer
            .client
            .iter()
            .find(|c| c.peer_id == peer_id)
            .unwrap();
        assert!(client.enabled);
        assert_eq!(client.name, Some(client_name.to_string()));

        // Verify persisted changes
        let content = std::fs::read_to_string(&config.config_file).unwrap();
        println!("Config content: {content}");
        assert!(content.contains(&peer_id.to_string()));
        assert!(content.contains(client_name));
    }

    #[test]
    fn test_remove_file_transfer_client() {
        let (config, _temp_dir) = create_temp_config();
        let peer_id = PeerId::random();

        // Add client first
        let config_with_client = config
            .add_file_transfer_client(true, peer_id, None)
            .unwrap();
        assert!(
            config_with_client
                .file_transfer
                .client
                .iter()
                .any(|c| c.peer_id == peer_id)
        );

        // Remove the client
        let final_config = config_with_client
            .remove_file_transfer_client(&peer_id)
            .unwrap();

        // Verify it's removed from memory
        assert!(
            !final_config
                .file_transfer
                .client
                .iter()
                .any(|c| c.peer_id == peer_id)
        );

        // Verify persisted changes
        let content = std::fs::read_to_string(&config.config_file).unwrap();
        println!("Config content: {content}");
        assert!(!content.contains(&peer_id.to_string()));
    }

    #[test]
    fn test_enable_file_transfer_client() {
        let (config, _temp_dir) = create_temp_config();
        let peer_id = PeerId::random();

        // Add client first with enabled=false
        let config_with_client = config
            .add_file_transfer_client(false, peer_id, None)
            .unwrap();

        // Enable the client
        let (final_config, client) = config_with_client
            .enable_file_transfer_client(&peer_id, true)
            .unwrap();

        let client = client.unwrap();

        // Verify memory updates
        assert!(client.enabled);
        let stored_client = final_config
            .file_transfer
            .client
            .iter()
            .find(|c| c.peer_id == peer_id)
            .unwrap();
        assert!(stored_client.enabled);

        // Verify persisted changes
        let content = std::fs::read_to_string(&config.config_file).unwrap();
        println!("Config content: {content}");
        assert!(content.contains(&peer_id.to_string()));
        assert!(content.contains("enabled = true"));
    }
}
