use std::path::PathBuf;

use libp2p_identity::PeerId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct FileTransfer {
    #[serde(default)]
    pub client: Vec<FileTransferClient>,
    #[serde(default)]
    pub server: FileTransferService,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FileTransferClient {
    pub target_peer: PeerId,

    pub proxy_ftp_host: String,
    pub proxy_ftp_port: u16,
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
