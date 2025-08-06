use std::sync::Arc;

use anyhow::{bail, Result};
use flutter_rust_bridge::frb;
use fungi_daemon::FungiDaemon;
use libp2p_identity::PeerId;
use once_cell::sync::Lazy;
use parking_lot::Mutex;

static FUNGI_DAEMON: Lazy<Mutex<Option<Arc<FungiDaemon>>>> = Lazy::new(Default::default);

pub struct FileTransferClient {
    pub enabled: bool,
    pub name: Option<String>,
    pub peer_id: String,
}

pub struct FtpProxy {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
}

pub struct WebdavProxy {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
}

pub struct ForwardingRule {
    pub local_host: String,
    pub local_port: u16,
    pub remote_peer_id: String,
    pub remote_port: u16,
}

pub struct ListeningRule {
    pub host: String,
    pub port: u16,
    pub allowed_peers: Vec<String>,
}

pub struct TcpTunnelingConfig {
    pub forwarding_enabled: bool,
    pub listening_enabled: bool,
    pub forwarding_rules: Vec<ForwardingRule>,
    pub listening_rules: Vec<ListeningRule>,
}

pub struct DeviceInfo {
    pub peer_id: String,
    pub hostname: Option<String>,
    pub os: String,
    pub version: String,
    pub ip_address: Option<String>,
}

pub struct PeerInfo {
    pub peer_id: String,
    pub hostname: Option<String>,
    pub public_ip: Option<String>,
    pub private_ips: Vec<String>,
    pub created_at: u64,
    pub last_connected: Option<u64>,
}

pub struct PeerWithInfo {
    pub peer_id: String,
    pub peer_info: Option<PeerInfo>,
}

impl From<fungi_config::file_transfer::FileTransferClient> for FileTransferClient {
    fn from(client: fungi_config::file_transfer::FileTransferClient) -> Self {
        Self {
            enabled: client.enabled,
            name: client.name,
            peer_id: client.peer_id.to_string(),
        }
    }
}

impl From<fungi_config::tcp_tunneling::ForwardingRule> for ForwardingRule {
    fn from(rule: fungi_config::tcp_tunneling::ForwardingRule) -> Self {
        Self {
            local_host: rule.local_host,
            local_port: rule.local_port,
            remote_peer_id: rule.remote_peer_id,
            remote_port: rule.remote_port,
        }
    }
}

impl From<fungi_config::tcp_tunneling::ListeningRule> for ListeningRule {
    fn from(rule: fungi_config::tcp_tunneling::ListeningRule) -> Self {
        Self {
            host: rule.host,
            port: rule.port,
            allowed_peers: Default::default(), // TODO: add support for allowed peers
        }
    }
}

impl From<fungi_daemon::DeviceInfo> for DeviceInfo {
    fn from(device: fungi_daemon::DeviceInfo) -> Self {
        Self {
            peer_id: device.peer_id().to_string(),
            hostname: device.hostname().map(|s| s.clone()),
            os: device.os().clone().into(),
            version: device.version().to_string(),
            ip_address: device.ip_address().map(|s| s.clone()),
        }
    }
}

impl From<fungi_config::known_peers::PeerInfo> for PeerInfo {
    fn from(peer: fungi_config::known_peers::PeerInfo) -> Self {
        Self {
            peer_id: peer.peer_id.to_string(),
            hostname: peer.hostname,
            public_ip: peer.public_ip,
            private_ips: peer.private_ips,
            created_at: peer.created_at,
            last_connected: peer.last_connected,
        }
    }
}

macro_rules! with_daemon {
    () => {{
        let Some(daemon) = FUNGI_DAEMON.lock().clone() else {
            bail!("Fungi daemon is not running.");
        };
        daemon
    }};
}

fn parse_peer_id(peer_id: String) -> Result<PeerId> {
    peer_id
        .parse::<PeerId>()
        .map_err(|_| anyhow::anyhow!("WrongPeerId"))
}

pub async fn start_fungi_daemon() -> Result<()> {
    if FUNGI_DAEMON.lock().is_some() {
        log::warn!("Fungi daemon is already running.");
        return Ok(());
    }

    let args = fungi_daemon::DaemonArgs::default();
    fungi_config::init(&args).unwrap();

    let daemon = fungi_daemon::FungiDaemon::start(args).await?;
    log::info!(
        "Fungi local peer id: {:?}",
        daemon.swarm_control().local_peer_id()
    );

    *FUNGI_DAEMON.lock() = Some(Arc::new(daemon));
    Ok(())
}

#[frb(sync)]
pub fn host_name() -> Option<String> {
    FungiDaemon::host_name()
}

#[frb(sync)]
pub fn peer_id() -> Result<String> {
    let daemon = with_daemon!();
    Ok(daemon.peer_id())
}

#[frb(sync)]
pub fn config_file_path() -> Result<String> {
    let daemon = with_daemon!();
    Ok(daemon.config_file_path())
}

#[frb(sync)]
pub fn get_incoming_allowed_peers_list() -> Result<Vec<String>> {
    let daemon = with_daemon!();
    Ok(daemon.get_incoming_allowed_peers_list())
}

#[frb(sync)]
pub fn add_incoming_allowed_peer(peer_id: String) -> Result<()> {
    let daemon = with_daemon!();
    daemon.add_incoming_allowed_peer(parse_peer_id(peer_id)?)
}

#[frb(sync)]
pub fn remove_incoming_allowed_peer(peer_id: String) -> Result<()> {
    let daemon = with_daemon!();
    daemon.remove_incoming_allowed_peer(parse_peer_id(peer_id)?)
}

#[frb(sync)]
pub fn get_file_transfer_service_enabled() -> Result<bool> {
    let daemon = with_daemon!();
    Ok(daemon.get_file_transfer_service_enabled())
}

#[frb(sync)]
pub fn get_file_transfer_service_root_dir() -> Result<String> {
    let daemon = with_daemon!();
    Ok(daemon
        .get_file_transfer_service_root_dir()
        .to_string_lossy()
        .to_string())
}

pub async fn start_file_transfer_service(root_dir: String) -> Result<()> {
    let daemon = with_daemon!();
    daemon.start_file_transfer_service(root_dir).await
}

#[frb(sync)]
pub fn stop_file_transfer_service() -> Result<()> {
    let daemon = with_daemon!();
    daemon.stop_file_transfer_service()
}

pub async fn add_file_transfer_client(
    enabled: bool,
    name: Option<String>,
    peer_id: String,
) -> Result<()> {
    let daemon = with_daemon!();
    daemon
        .add_file_transfer_client(enabled, name, parse_peer_id(peer_id)?)
        .await
}

#[frb(sync)]
pub fn remove_file_transfer_client(peer_id: String) -> Result<()> {
    let daemon = with_daemon!();
    daemon.remove_file_transfer_client(parse_peer_id(peer_id)?)
}

pub async fn enable_file_transfer_client(peer_id: String, enabled: bool) -> Result<()> {
    let daemon = with_daemon!();
    daemon
        .enable_file_transfer_client(parse_peer_id(peer_id)?, enabled)
        .await
}

#[frb(sync)]
pub fn get_all_file_transfer_clients() -> Result<Vec<FileTransferClient>> {
    let daemon = with_daemon!();
    Ok(daemon
        .get_all_file_transfer_clients()
        .into_iter()
        .map(FileTransferClient::from)
        .collect())
}

#[frb(sync)]
pub fn get_ftp_proxy() -> Result<FtpProxy> {
    let daemon = with_daemon!();
    let proxy = daemon.get_ftp_proxy();
    Ok(FtpProxy {
        enabled: proxy.enabled,
        host: proxy.host.to_string(),
        port: proxy.port,
    })
}

#[frb(sync)]
pub fn update_ftp_proxy(enabled: bool, host: String, port: u16) -> Result<()> {
    let daemon = with_daemon!();
    daemon.update_ftp_proxy(enabled, host.parse()?, port)
}

#[frb(sync)]
pub fn get_webdav_proxy() -> Result<WebdavProxy> {
    let daemon = with_daemon!();
    let proxy = daemon.get_webdav_proxy();
    Ok(WebdavProxy {
        enabled: proxy.enabled,
        host: proxy.host.to_string(),
        port: proxy.port,
    })
}

#[frb(sync)]
pub fn update_webdav_proxy(enabled: bool, host: String, port: u16) -> Result<()> {
    let daemon = with_daemon!();
    daemon.update_webdav_proxy(enabled, host.parse()?, port)
}

// TCP Tunneling API methods
#[frb(sync)]
pub fn get_tcp_tunneling_config() -> Result<TcpTunnelingConfig> {
    let daemon = with_daemon!();
    let config = daemon.get_tcp_tunneling_config();

    let forwarding_rules = daemon
        .get_tcp_forwarding_rules()
        .into_iter()
        .map(|(_, rule)| rule.into())
        .collect();

    let listening_rules = daemon
        .get_tcp_listening_rules()
        .into_iter()
        .map(|(_, rule)| rule.into())
        .collect();

    Ok(TcpTunnelingConfig {
        forwarding_enabled: config.forwarding.enabled,
        listening_enabled: config.listening.enabled,
        forwarding_rules,
        listening_rules,
    })
}

pub async fn add_tcp_forwarding_rule(
    local_host: String,
    local_port: u16,
    peer_id: String,
    remote_port: u16,
) -> Result<String> {
    let daemon = with_daemon!();
    daemon
        .add_tcp_forwarding_rule(local_host, local_port, peer_id, remote_port)
        .await
}

#[frb(sync)]
pub fn remove_tcp_forwarding_rule(rule_id: String) -> Result<()> {
    let daemon = with_daemon!();
    daemon.remove_tcp_forwarding_rule(rule_id)
}

pub async fn add_tcp_listening_rule(
    local_host: String,
    local_port: u16,
    allowed_peers: Vec<String>,
) -> Result<String> {
    let daemon = with_daemon!();
    daemon
        .add_tcp_listening_rule(local_host, local_port, allowed_peers)
        .await
}

#[frb(sync)]
pub fn remove_tcp_listening_rule(rule_id: String) -> Result<()> {
    let daemon = with_daemon!();
    daemon.remove_tcp_listening_rule(rule_id)
}

pub async fn get_local_devices() -> Result<Vec<DeviceInfo>> {
    let daemon = with_daemon!();
    let devices = daemon.get_local_devices().await?;
    Ok(devices.into_iter().map(|d| d.into()).collect())
}

#[frb(sync)]
pub fn get_all_known_peers() -> Result<Vec<PeerInfo>> {
    let daemon = with_daemon!();
    let peers = daemon.get_all_known_peers();
    Ok(peers.into_iter().map(|p| p.into()).collect())
}

#[frb(sync)]
pub fn add_or_update_known_peer(peer_id: String, hostname: Option<String>) -> Result<()> {
    let daemon = with_daemon!();
    daemon.add_or_update_known_peer(parse_peer_id(peer_id)?, hostname)
}

#[frb(sync)]
pub fn get_known_peer_info(peer_id: String) -> Result<Option<PeerInfo>> {
    let daemon = with_daemon!();
    Ok(daemon
        .get_known_peer_info(parse_peer_id(peer_id)?)
        .map(|p| p.into()))
}

#[frb(sync)]
pub fn remove_known_peer(peer_id: String) -> Result<()> {
    let daemon = with_daemon!();
    daemon.remove_known_peer(parse_peer_id(peer_id)?)
}

#[frb(sync)]
pub fn get_incoming_allowed_peers_with_info() -> Result<Vec<PeerWithInfo>> {
    let daemon = with_daemon!();
    let peers_with_info = daemon.get_incoming_allowed_peers_with_info();
    Ok(peers_with_info
        .into_iter()
        .map(|(peer_id, peer_info)| PeerWithInfo {
            peer_id,
            peer_info: peer_info.map(|p| p.into()),
        })
        .collect())
}

#[flutter_rust_bridge::frb(init)]
pub fn init_app() {
    // Default utilities - feel free to customize
    // flutter_rust_bridge::setup_default_user_utils();
    env_logger::init();
}
