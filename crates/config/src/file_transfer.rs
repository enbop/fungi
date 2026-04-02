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
            enabled: false,
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
            enabled: false,
            host: "127.0.0.1".parse().unwrap(),
            port: DEFAULT_WEBDAV_PORT,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ftp_proxy_default_is_enabled_on_loopback() {
        let ftp = FtpProxy::default();
        assert!(ftp.enabled);
        assert_eq!(ftp.host.to_string(), "127.0.0.1");
        assert_eq!(ftp.port, 2121);
    }

    #[test]
    fn webdav_proxy_default_is_enabled_on_loopback() {
        let webdav = WebdavProxy::default();
        assert!(webdav.enabled);
        assert_eq!(webdav.host.to_string(), "127.0.0.1");
        assert_eq!(webdav.port, 8181);
    }

    #[test]
    fn file_transfer_default_has_no_clients() {
        let ft = FileTransfer::default();
        assert!(ft.client.is_empty());
    }

    #[test]
    fn file_transfer_service_default_is_disabled_with_empty_path() {
        let svc = FileTransferService::default();
        assert!(!svc.enabled);
        assert_eq!(svc.shared_root_dir, PathBuf::new());
    }

    #[test]
    fn ftp_proxy_deserializes_custom_port() {
        let toml = "[proxy_ftp]\nenabled = true\nhost = \"127.0.0.1\"\nport = 3000\n";
        let config: toml::Value = toml::from_str(toml).unwrap();
        let ftp: FtpProxy = config["proxy_ftp"].clone().try_into().unwrap();
        assert_eq!(ftp.port, 3000);
    }

    #[test]
    fn webdav_proxy_deserializes_disabled() {
        let toml = "[proxy_webdav]\nenabled = false\nhost = \"127.0.0.1\"\nport = 8181\n";
        let config: toml::Value = toml::from_str(toml).unwrap();
        let webdav: WebdavProxy = config["proxy_webdav"].clone().try_into().unwrap();
        assert!(!webdav.enabled);
    }

    #[test]
    fn file_transfer_service_deserializes_enabled_with_path() {
        let toml =
            "[server]\nenabled = true\nshared_root_dir = \"/home/user/files\"\n";
        let config: toml::Value = toml::from_str(toml).unwrap();
        let svc: FileTransferService = config["server"].clone().try_into().unwrap();
        assert!(svc.enabled);
        assert_eq!(svc.shared_root_dir, PathBuf::from("/home/user/files"));
    }
}
