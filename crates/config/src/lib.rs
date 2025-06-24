pub mod file_transfer;
mod init;
mod libp2p;
mod tcp_tunneling;

pub use init::init;

use anyhow::Result;
use libp2p::*;
use libp2p_identity::PeerId;
use serde::{Deserialize, Serialize};
use std::{
    io,
    path::{Path, PathBuf},
};
use tcp_tunneling::*;
use toml_edit::DocumentMut;

use crate::file_transfer::{FileTransfer, FileTransferClient, FileTransferService};

pub const DEFAULT_CONFIG_FILE: &str = "config.toml";
pub const DEFAULT_FUNGI_DIR: &str = ".fungi";
pub const DEFAULT_IPC_DIR_NAME: &str = ".ipc";
pub const DEFAULT_DAEMON_RPC_NAME: &str = ".fungi_daemon.sock";

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct FungiConfig {
    #[serde(default)]
    pub network: Network,
    #[serde(default)]
    pub tcp_tunneling: TcpTunneling,
    #[serde(default)]
    pub file_transfer: FileTransfer,

    #[serde(skip)]
    config_file: PathBuf,
    #[serde(skip)]
    document: DocumentMut,
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

    pub fn apply_from_dir(fungi_dir: &Path) -> Result<Self> {
        let config_file = fungi_dir.join(DEFAULT_CONFIG_FILE);
        if !config_file.exists() {
            Self::init_config_file(config_file.clone())?;
        }

        println!("Loading Fungi config from: {:?}", config_file);
        let s = std::fs::read_to_string(&config_file)?;
        let mut cfg = Self::parse_toml(&s)?;
        cfg.config_file = config_file;
        Ok(cfg)
    }

    pub fn parse_toml(s: &str) -> Result<Self> {
        let document: DocumentMut = s.parse()?;
        let mut config: Self = toml::from_str(s)?;
        config.document = document;
        Ok(config)
    }

    fn save(&self) -> io::Result<()> {
        let toml_string = self.document.to_string();
        std::fs::write(&self.config_file, toml_string)?;
        Ok(())
    }

    pub fn add_incoming_allowed_peer(&mut self, peer_id: &PeerId) -> Result<()> {
        if self.network.incoming_allowed_peers.contains(peer_id) {
            return Ok(());
        }
        self.network.incoming_allowed_peers.push(peer_id.clone());
        // TODO remove this unwrap
        self.document["network"]["incoming_allowed_peers"]
            .as_array_mut()
            .unwrap()
            .push(peer_id.to_string());
        self.save()?;
        Ok(())
    }

    pub fn remove_incoming_allowed_peer(&mut self, peer_id: &PeerId) -> Result<()> {
        if let Some(pos) = self
            .network
            .incoming_allowed_peers
            .iter()
            .position(|p| p == peer_id)
        {
            self.network.incoming_allowed_peers.remove(pos);

            let peer_id_str = peer_id.to_string();
            // TODO remove this unwrap
            self.document["network"]["incoming_allowed_peers"]
                .as_array_mut()
                .unwrap()
                .retain(|v| v.as_str() != Some(&peer_id_str));
            self.save()?;
        }
        Ok(())
    }

    pub fn update_file_transfer_service(
        &mut self,
        enabled: bool,
        root_dir: PathBuf,
    ) -> Result<FileTransferService> {
        self.file_transfer.server.enabled = enabled;
        self.file_transfer.server.shared_root_dir = root_dir.clone();

        self.document["file_transfer"]["server"]["enabled"] = toml_edit::value(enabled);
        self.document["file_transfer"]["server"]["shared_root_dir"] =
            toml_edit::value(root_dir.to_string_lossy().to_string());
        self.save()?;
        Ok(self.file_transfer.server.clone())
    }

    pub fn add_file_transfer_client(
        &mut self,
        enabled: bool,
        peer_id: PeerId,
        name: Option<String>,
    ) -> Result<()> {
        if self
            .file_transfer
            .client
            .iter()
            .any(|c| c.peer_id == peer_id)
        {
            return Ok(());
        }
        let client = FileTransferClient {
            enabled,
            name: name.clone(),
            peer_id,
        };
        self.file_transfer.client.push(client.clone());

        // TODO remove this unwrap
        let client_array = self.document["file_transfer"]["client"]
            .as_array_mut()
            .unwrap();

        let mut table = toml_edit::InlineTable::new();
        table.insert("enabled", enabled.into());
        table.insert("peer_id", peer_id.to_string().into());
        if let Some(name_str) = &name {
            table.insert("name", name_str.into());
        }

        client_array.push(toml_edit::Value::InlineTable(table));
        self.save()?;
        Ok(())
    }

    pub fn remove_file_transfer_client(&mut self, peer_id: &PeerId) -> Result<()> {
        if let Some(pos) = self
            .file_transfer
            .client
            .iter()
            .position(|c| c.peer_id == *peer_id)
        {
            self.file_transfer.client.remove(pos);

            // TODO remove this unwrap
            let client_array = self.document["file_transfer"]["client"]
                .as_array_mut()
                .unwrap();
            client_array.retain(|v| {
                v.as_inline_table()
                    .and_then(|t| t.get("peer_id"))
                    .and_then(|v| v.as_str())
                    != Some(&peer_id.to_string())
            });
            self.save()?;
        }
        Ok(())
    }

    pub fn enable_file_transfer_client(
        &mut self,
        peer_id: &PeerId,
        enabled: bool,
    ) -> Result<Option<FileTransferClient>> {
        let client = {
            let Some(client) = self
                .file_transfer
                .client
                .iter_mut()
                .find(|c| c.peer_id == *peer_id)
            else {
                return Ok(None);
            };

            client.enabled = enabled;
            client.clone()
        };

        // Update the document
        if let Some(client_array) = self.document["file_transfer"]["client"].as_array_mut() {
            for item in client_array.iter_mut() {
                let Some(inline_table) = item.as_inline_table_mut() else {
                    continue;
                };
                let Some(id) = inline_table.get_mut("peer_id") else {
                    continue;
                };
                if id.as_str() == Some(&peer_id.to_string()) {
                    inline_table.insert("enabled", enabled.into());
                    break;
                }
            }
        }
        self.save()?;
        Ok(Some(client))
    }
}

pub trait FungiDir {
    fn fungi_dir(&self) -> PathBuf;

    fn ipc_dir(&self) -> PathBuf {
        let dir = self.fungi_dir().join(DEFAULT_IPC_DIR_NAME);
        if !dir.exists() {
            std::fs::create_dir(&dir).unwrap();
        }
        dir
    }

    fn daemon_rpc_path(&self) -> PathBuf {
        self.ipc_dir().join(DEFAULT_DAEMON_RPC_NAME)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use toml_edit::value;

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
        let (mut config, _path) = create_temp_config();
        let peer_id = PeerId::random();
        config.add_incoming_allowed_peer(&peer_id).unwrap();
        assert!(config.network.incoming_allowed_peers.contains(&peer_id));
        assert!(
            config.document["network"]["incoming_allowed_peers"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item.as_str() == Some(&peer_id.to_string()))
        );
        assert!(config.config_file.exists());
        let content = std::fs::read_to_string(&config.config_file).unwrap();
        println!("Config content: {}", content);
        assert!(content.contains(&peer_id.to_string()));
    }

    #[test]
    fn test_remove_incoming_allowed_peer() {
        let (mut config, _temp_dir) = create_temp_config();

        // Add a peer first
        let peer_id = PeerId::random();
        config.add_incoming_allowed_peer(&peer_id).unwrap();
        assert!(config.network.incoming_allowed_peers.contains(&peer_id));

        // Remove it
        config.remove_incoming_allowed_peer(&peer_id).unwrap();

        // Verify it's removed from memory
        assert!(!config.network.incoming_allowed_peers.contains(&peer_id));

        // Verify it's removed from document
        assert!(
            !config.document["network"]["incoming_allowed_peers"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item.as_str() == Some(&peer_id.to_string()))
        );

        // Verify changes were persisted
        let content = std::fs::read_to_string(&config.config_file).unwrap();
        assert!(!content.contains(&peer_id.to_string()));
    }

    #[test]
    fn test_update_file_transfer_service() {
        let (mut config, _temp_dir) = create_temp_config();
        let root_dir = PathBuf::from("/test/path");

        let service = config
            .update_file_transfer_service(true, root_dir.clone())
            .unwrap();

        // Verify memory updates
        assert!(service.enabled);
        assert_eq!(service.shared_root_dir, root_dir);
        assert!(config.file_transfer.server.enabled);
        assert_eq!(config.file_transfer.server.shared_root_dir, root_dir);

        // Verify document updates
        assert_eq!(
            config.document["file_transfer"]["server"]["enabled"].as_bool(),
            Some(true)
        );
        assert_eq!(
            config.document["file_transfer"]["server"]["shared_root_dir"].as_str(),
            Some(root_dir.to_str().unwrap())
        );

        // Verify persisted changes
        let content = std::fs::read_to_string(&config.config_file).unwrap();
        println!("Config content: {}", content);
        assert!(content.contains(&format!(
            "shared_root_dir = \"{}\"",
            root_dir.to_string_lossy()
        )));
        assert!(content.contains("enabled = true"));
    }

    #[test]
    fn test_add_file_transfer_client() {
        let (mut config, _temp_dir) = create_temp_config();
        let peer_id = PeerId::random();
        let client_name = "test-client";

        config
            .add_file_transfer_client(true, peer_id.clone(), Some(client_name.to_string()))
            .unwrap();

        // Verify memory updates
        let client = config
            .file_transfer
            .client
            .iter()
            .find(|c| c.peer_id == peer_id)
            .unwrap();
        assert!(client.enabled);
        assert_eq!(client.name, Some(client_name.to_string()));

        // Verify document updates
        let client_array = config.document["file_transfer"]["client"]
            .as_array()
            .unwrap();
        let client_exists = client_array.iter().any(|item| {
            let item = item.as_inline_table().unwrap();
            item.get("peer_id")
                .and_then(|v| v.as_str())
                .map(|s| s == peer_id.to_string())
                .unwrap_or(false)
        });
        assert!(client_exists);

        // Verify persisted changes
        let content = std::fs::read_to_string(&config.config_file).unwrap();
        println!("Config content: {}", content);
        assert!(content.contains(&peer_id.to_string()));
        assert!(content.contains(client_name));
    }

    #[test]
    fn test_remove_file_transfer_client() {
        let (mut config, _temp_dir) = create_temp_config();
        let peer_id = PeerId::random();

        // Add client first
        config
            .add_file_transfer_client(true, peer_id.clone(), None)
            .unwrap();
        assert!(
            config
                .file_transfer
                .client
                .iter()
                .any(|c| c.peer_id == peer_id)
        );

        // Remove the client
        config.remove_file_transfer_client(&peer_id).unwrap();

        // Verify it's removed from memory
        assert!(
            !config
                .file_transfer
                .client
                .iter()
                .any(|c| c.peer_id == peer_id)
        );

        // Verify it's removed from document
        let client_array = config.document["file_transfer"]["client"]
            .as_array()
            .unwrap();
        let client_exists = client_array.iter().any(|item| {
            item.as_inline_table()
                .and_then(|t| t.get("peer_id"))
                .and_then(|v| v.as_str())
                .map(|s| s == peer_id.to_string())
                .unwrap_or(false)
        });
        assert!(!client_exists);

        // Verify persisted changes
        let content = std::fs::read_to_string(&config.config_file).unwrap();
        println!("Config content: {}", content);
        assert!(!content.contains(&peer_id.to_string()));
    }

    #[test]
    fn test_enable_file_transfer_client() {
        let (mut config, _temp_dir) = create_temp_config();
        let peer_id = PeerId::random();

        // Add client first with enabled=false
        config
            .add_file_transfer_client(false, peer_id.clone(), None)
            .unwrap();

        // Enable the client
        let client = config
            .enable_file_transfer_client(&peer_id, true)
            .unwrap()
            .unwrap();

        // Verify memory updates
        assert!(client.enabled);
        let stored_client = config
            .file_transfer
            .client
            .iter()
            .find(|c| c.peer_id == peer_id)
            .unwrap();
        assert!(stored_client.enabled);

        // Verify document updates
        let client_array = config.document["file_transfer"]["client"]
            .as_array()
            .unwrap();
        for item in client_array.iter() {
            if let Some(table) = item.as_inline_table() {
                if let Some(id) = table.get("peer_id").and_then(|v| v.as_str()) {
                    if id == peer_id.to_string() {
                        assert_eq!(table.get("enabled").and_then(|v| v.as_bool()), Some(true));
                        break;
                    }
                }
            }
        }

        // Verify persisted changes
        let content = std::fs::read_to_string(&config.config_file).unwrap();
        println!("Config content: {}", content);
        assert!(content.contains(&peer_id.to_string()));
        assert!(content.contains("enabled = true"));
    }
}
