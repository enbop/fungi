mod build_info;
pub mod devices;
pub mod direct_addresses;
pub mod file_transfer;
mod init;
mod libp2p;
pub mod local_access;
pub mod paths;
mod rpc;
pub mod runtime;
pub mod service_cache;
pub mod tcp_tunneling;
pub mod trusted_devices;

pub use crate::libp2p::*;
pub use build_info::{
    NIGHTLY_CHANNEL, NIGHTLY_FUNGI_DIR, NIGHTLY_RPC_ADDRESS, STABLE_CHANNEL, STABLE_FUNGI_DIR,
    STABLE_RPC_ADDRESS, build_commit, build_time, default_fungi_dir_name, default_rpc_address,
    dist_channel,
};
pub use fungi_config_migrate::{
    DetectedVersion as FungiDirDetectedVersion, MigrationReport, migrate_if_needed,
};
pub use init::init;

use anyhow::{Context as _, Result};
use libp2p_identity::PeerId;
use multiaddr::Multiaddr;
use serde::{Deserialize, Serialize};
use std::{
    net::IpAddr,
    path::{Path, PathBuf},
};
use tcp_tunneling::*;

use crate::{
    file_transfer::{FileTransfer, FileTransferClient, FileTransferService},
    runtime::Runtime,
};

pub const DEFAULT_CONFIG_FILE: &str = "config.toml";
pub const DEFAULT_FUNGI_DIR: &str = NIGHTLY_FUNGI_DIR;
pub const DEFAULT_RPC_ADDRESS: &str = NIGHTLY_RPC_ADDRESS;
pub const CURRENT_CONFIG_VERSION: u32 = fungi_config_migrate::CURRENT_FUNGI_DIR_VERSION;

fn default_config_version() -> u32 {
    CURRENT_CONFIG_VERSION
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FungiConfig {
    #[serde(default = "default_config_version")]
    pub version: u32,
    #[serde(default)]
    pub rpc: rpc::Rpc,
    #[serde(default)]
    pub network: Network,
    #[serde(default)]
    pub tcp_tunneling: TcpTunneling,
    #[serde(default)]
    pub file_transfer: FileTransfer,
    #[serde(default)]
    pub runtime: Runtime,

    #[serde(default)]
    custom_hostname: Option<String>,

    #[serde(skip)]
    config_file: PathBuf,
}

impl Default for FungiConfig {
    fn default() -> Self {
        Self {
            version: CURRENT_CONFIG_VERSION,
            rpc: rpc::Rpc::default(),
            network: Network::default(),
            tcp_tunneling: TcpTunneling::default(),
            file_transfer: FileTransfer::default(),
            runtime: Runtime::default(),
            custom_hostname: None,
            config_file: PathBuf::new(),
        }
    }
}

impl FungiConfig {
    pub fn config_file_path(&self) -> &Path {
        &self.config_file
    }

    pub fn init_config_file(config_file: PathBuf) -> Result<()> {
        if config_file.exists() {
            return Ok(());
        }
        let config = FungiConfig::default_for_dir();
        let toml_string = toml::to_string(&config)?;
        std::fs::write(&config_file, toml_string)?;
        Ok(())
    }

    fn default_for_dir() -> Self {
        let mut config = FungiConfig::default();
        config.apply_config_defaults();
        config
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
        cfg.apply_config_defaults();
        cfg.config_file = config_file;
        Ok(cfg)
    }

    pub fn apply_from_dir(fungi_dir: &Path) -> Result<Self> {
        let config_file = fungi_dir.join(DEFAULT_CONFIG_FILE);
        if !config_file.exists() {
            Self::init_config_file(config_file.clone())?;
        }

        println!("Loading Fungi config from: {config_file:?}");
        let s = std::fs::read_to_string(&config_file).context("Failed to read config file")?;
        let mut cfg = Self::parse_toml(&s)?;
        cfg.apply_config_defaults();
        cfg.config_file = config_file;
        Ok(cfg)
    }

    pub fn parse_toml(s: &str) -> Result<Self> {
        let config: Self = toml::from_str(s).context("Failed to parse config file")?;
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

    pub fn get_runtime_config(&self) -> Runtime {
        self.runtime.clone()
    }

    pub fn add_runtime_allowed_host_path(&self, path: PathBuf) -> Result<Self> {
        let fungi_dir = self.config_file.parent().unwrap_or_else(|| Path::new("."));
        let path = Runtime::validate_allowed_host_path(&path, fungi_dir)?;

        self.update_and_save(|config| {
            if !config
                .runtime
                .allowed_host_paths
                .iter()
                .any(|entry| entry == &path)
            {
                config.runtime.allowed_host_paths.push(path.clone());
                config.runtime.allowed_host_paths.sort();
            }
        })
    }

    pub fn remove_runtime_allowed_host_path(&self, path: &Path) -> Result<Self> {
        self.update_and_save(|config| {
            config
                .runtime
                .allowed_host_paths
                .retain(|entry| entry != path);
        })
    }

    pub fn set_relay_enabled(&self, enabled: bool) -> Result<Self> {
        self.update_and_save(|config| {
            config.network.relay_enabled = enabled;
        })
    }

    pub fn set_use_community_relays(&self, enabled: bool) -> Result<Self> {
        self.update_and_save(|config| {
            config.network.use_community_relays = enabled;
        })
    }

    pub fn add_custom_relay_address(&self, address: Multiaddr) -> Result<Self> {
        self.update_and_save(|config| {
            if !config.network.custom_relay_addresses.contains(&address) {
                config.network.custom_relay_addresses.push(address);
            }
        })
    }

    pub fn remove_custom_relay_address(&self, address: &Multiaddr) -> Result<Self> {
        self.update_and_save(|config| {
            config
                .network
                .custom_relay_addresses
                .retain(|entry| entry != address);
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
                && r.remote_protocol == rule.remote_protocol
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
                    && r.remote_protocol == rule.remote_protocol
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
            .any(|r| r.host == rule.host && r.port == rule.port && r.protocol == rule.protocol);

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
            config.tcp_tunneling.listening.rules.retain(|r| {
                !(r.host == rule.host && r.port == rule.port && r.protocol == rule.protocol)
            });
        })
    }
}

impl FungiConfig {
    fn apply_config_defaults(&mut self) {
        if self.version == 0 {
            self.version = CURRENT_CONFIG_VERSION;
        }
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
        assert!(content.contains("version = 2"));
        assert!(content.contains("[network]"));
        assert!(content.contains("[tcp_tunneling"));
        assert!(content.contains("[file_transfer"));
        assert!(content.contains("[runtime]"));
        assert!(!content.contains("allowed_ports"));
        assert!(!content.contains("allowed_port_ranges"));
        assert!(!content.contains("allowed_host_paths"));
        assert!(!content.contains("[service_cache"));
    }

    #[test]
    fn test_default_network_relay_settings() {
        let (config, _temp_dir) = create_temp_config();

        assert!(config.network.relay_enabled);
        assert!(config.network.use_community_relays);
        assert!(config.network.custom_relay_addresses.is_empty());
    }

    #[test]
    fn test_file_transfer_defaults_disabled() {
        let (config, _temp_dir) = create_temp_config();

        assert!(!config.file_transfer.server.enabled);
        assert!(!config.file_transfer.proxy_ftp.enabled);
        assert!(!config.file_transfer.proxy_webdav.enabled);
    }

    #[test]
    fn test_set_relay_enabled_persists() {
        let (config, _temp_dir) = create_temp_config();

        let updated = config.set_relay_enabled(false).unwrap();

        assert!(!updated.network.relay_enabled);
        let content = std::fs::read_to_string(&config.config_file).unwrap();
        assert!(content.contains("relay_enabled = false"));
    }

    #[test]
    fn test_set_use_community_relays_persists() {
        let (config, _temp_dir) = create_temp_config();

        let updated = config.set_use_community_relays(false).unwrap();

        assert!(!updated.network.use_community_relays);
        let content = std::fs::read_to_string(&config.config_file).unwrap();
        assert!(content.contains("use_community_relays = false"));
    }

    #[test]
    fn test_add_and_remove_custom_relay_address_persist() {
        let (config, _temp_dir) = create_temp_config();
        let address: Multiaddr =
            "/ip4/127.0.0.1/tcp/30001/p2p/16Uiu2HAmGXFS6aYsKKYRkEDo1tNigZKN8TAYrsfSnEdC5sZLNkiE"
                .parse()
                .unwrap();

        let updated = config.add_custom_relay_address(address.clone()).unwrap();
        assert_eq!(
            updated.network.custom_relay_addresses,
            vec![address.clone()]
        );

        let deduped = updated.add_custom_relay_address(address.clone()).unwrap();
        assert_eq!(
            deduped.network.custom_relay_addresses,
            vec![address.clone()]
        );

        let final_config = deduped.remove_custom_relay_address(&address).unwrap();
        assert!(final_config.network.custom_relay_addresses.is_empty());

        let content = std::fs::read_to_string(&config.config_file).unwrap();
        assert!(!content.contains(&address.to_string()));
    }

    #[test]
    fn test_effective_relay_addresses_follow_flags() {
        let mut network = Network::default();
        let community: Multiaddr =
            "/ip4/10.0.0.1/tcp/30001/p2p/16Uiu2HAmGXFS6aYsKKYRkEDo1tNigZKN8TAYrsfSnEdC5sZLNkiE"
                .parse()
                .unwrap();
        let custom: Multiaddr =
            "/ip4/10.0.0.2/tcp/30001/p2p/16Uiu2HAmGXFS6aYsKKYRkEDo1tNigZKN8TAYrsfSnEdC5sZLNkiE"
                .parse()
                .unwrap();

        network.custom_relay_addresses.push(custom.clone());

        let effective = network.effective_relay_addresses(std::slice::from_ref(&community));
        assert_eq!(effective.len(), 2);

        network.use_community_relays = false;
        let effective = network.effective_relay_addresses(std::slice::from_ref(&community));
        assert_eq!(effective.len(), 1);
        assert_eq!(effective[0].address, custom);

        network.relay_enabled = false;
        let effective = network.effective_relay_addresses(&[community]);
        assert!(effective.is_empty());
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
