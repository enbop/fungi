use std::path::PathBuf;

use libp2p_identity::PeerId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct FileTransfer {
    #[serde(default)]
    pub client: Vec<FileTransferClient>,
    #[serde(default)]
    pub server: FileTransferService,
    #[serde(default)]
    pub proxy_ftp: ProxyFtp,
    #[serde(default)]
    pub proxy_webdav: ProxyWebdav,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FileTransferClient {
    #[serde(default)]
    pub name: Option<String>,
    pub peer_id: PeerId,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct FileTransferService {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub allowed_peers: Vec<PeerId>,
    #[serde(default)]
    pub shared_root_dir: PathBuf,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ProxyFtp {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub port: u16,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ProxyWebdav {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub port: u16,
}
