use std::{net::IpAddr, path::PathBuf};

use libp2p_identity::PeerId;
use serde::{Deserialize, Serialize};

const DEFAULT_FTP_PORT: u16 = 2121;
const DEFAULT_WEBDAV_PORT: u16 = 8181;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct FileTransfer {
    #[serde(default)]
    pub client: Vec<FileTransferClient>,
    #[serde(default)]
    pub server: FileTransferService,
    #[serde(default)]
    pub proxy_ftp: FtpProxy,
    #[serde(default)]
    pub proxy_webdav: WebdavProxy,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FileTransferClient {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub name: Option<String>,
    pub peer_id: PeerId,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct FileTransferService {
    #[serde(default)]
    pub enabled: bool,
    // #[serde(default)]
    // pub allowed_peers: Vec<PeerId>,
    #[serde(default)]
    pub shared_root_dir: PathBuf,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FtpProxy {
    pub enabled: bool,
    pub host: IpAddr,
    pub port: u16,
}

impl Default for FtpProxy {
    fn default() -> Self {
        Self {
            enabled: true,
            host: "127.0.0.1".parse().unwrap(),
            port: DEFAULT_FTP_PORT,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WebdavProxy {
    pub enabled: bool,
    pub host: IpAddr,
    pub port: u16,
}

impl Default for WebdavProxy {
    fn default() -> Self {
        Self {
            enabled: true,
            host: "127.0.0.1".parse().unwrap(),
            port: DEFAULT_WEBDAV_PORT,
        }
    }
}
