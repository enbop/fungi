use std::sync::Arc;

use anyhow::{bail, Result};
use flutter_rust_bridge::frb;
use fungi_daemon::FungiDaemon;
use libp2p_identity::PeerId;
use once_cell::sync::Lazy;
use parking_lot::Mutex;

static FUNGI_DAEMON: Lazy<Mutex<Option<Arc<FungiDaemon>>>> = Lazy::new(|| Default::default());

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

pub struct LocalSocket {
    pub host: String,
    pub port: u16,
}

pub struct ForwardingRuleRemote {
    pub peer_id: String,
    pub port: u16,
}

pub struct ForwardingRule {
    pub local_socket: LocalSocket,
    pub remote: ForwardingRuleRemote,
}

pub struct ListeningRule {
    pub local_socket: LocalSocket,
    pub allowed_peers: Vec<String>,
}

pub struct TcpTunnelingConfig {
    pub forwarding_enabled: bool,
    pub listening_enabled: bool,
    pub forwarding_rules: Vec<ForwardingRule>,
    pub listening_rules: Vec<ListeningRule>,
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

impl From<fungi_config::tcp_tunneling::LocalSocket> for LocalSocket {
    fn from(socket: fungi_config::tcp_tunneling::LocalSocket) -> Self {
        Self {
            host: socket.host,
            port: socket.port,
        }
    }
}

impl From<fungi_config::tcp_tunneling::ForwardingRuleRemote> for ForwardingRuleRemote {
    fn from(remote: fungi_config::tcp_tunneling::ForwardingRuleRemote) -> Self {
        Self {
            peer_id: remote.peer_id,
            port: remote.port,
        }
    }
}

impl From<fungi_config::tcp_tunneling::ForwardingRule> for ForwardingRule {
    fn from(rule: fungi_config::tcp_tunneling::ForwardingRule) -> Self {
        Self {
            local_socket: rule.local_socket.into(),
            remote: rule.remote.into(),
        }
    }
}

impl From<fungi_config::tcp_tunneling::ListeningRule> for ListeningRule {
    fn from(rule: fungi_config::tcp_tunneling::ListeningRule) -> Self {
        Self {
            local_socket: rule.local_socket.into(),
            allowed_peers: rule.allowed_peers,
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

#[flutter_rust_bridge::frb(init)]
pub fn init_app() {
    // Default utilities - feel free to customize
    // flutter_rust_bridge::setup_default_user_utils();
    env_logger::init();
}
