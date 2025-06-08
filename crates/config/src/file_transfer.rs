use std::path::PathBuf;

use libp2p_identity::PeerId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct FileTransfer {
    #[serde(default)]
    pub client: Vec<FileTransferClient>,
    #[serde(default)]
    pub server: FileTransferServer,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FileTransferClient {
    pub target_peer: PeerId,

    pub proxy_ftp_port: u16,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct FileTransferServer {
    #[serde(default)]
    pub enable: bool,
    #[serde(default)]
    pub allowed_peers: Vec<PeerId>,
    #[serde(default)]
    pub shared_root_dir: PathBuf,
}
