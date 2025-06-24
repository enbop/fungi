use std::sync::Arc;

use anyhow::{bail, Result};
use flutter_rust_bridge::frb;
use fungi_daemon::FungiDaemon;
use once_cell::sync::Lazy;
use parking_lot::Mutex;

static FUNGI_DAEMON: Lazy<Mutex<Option<Arc<FungiDaemon>>>> = Lazy::new(|| Default::default());

macro_rules! with_daemon {
    () => {{
        let Some(daemon) = FUNGI_DAEMON.lock().clone() else {
            bail!("Fungi daemon is not running.");
        };
        daemon
    }};
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
    daemon.add_incoming_allowed_peer(peer_id.parse()?)
}

#[frb(sync)]
pub fn remove_incoming_allowed_peer(peer_id: String) -> Result<()> {
    let daemon = with_daemon!();
    daemon.remove_incoming_allowed_peer(peer_id.parse()?)
}

#[frb(sync)]
pub fn get_file_transfer_service_root_dir() -> Result<String> {
    let daemon = with_daemon!();
    Ok(daemon
        .get_file_transfer_service_root_dir()
        .to_string_lossy()
        .to_string())
}

#[frb(sync)]
pub fn start_file_transfer_service(root_dir: String) -> Result<()> {
    let daemon = with_daemon!();
    daemon.start_file_transfer_service(root_dir)
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
        .add_file_transfer_client(enabled, name, peer_id.parse()?)
        .await
}

#[frb(sync)]
pub fn remove_file_transfer_client(peer_id: String) -> Result<()> {
    let daemon = with_daemon!();
    daemon.remove_file_transfer_client(peer_id.parse()?)
}

pub async fn enable_file_transfer_client(peer_id: String, enabled: bool) -> Result<()> {
    let daemon = with_daemon!();
    daemon
        .enable_file_transfer_client(peer_id.parse()?, enabled)
        .await
}

#[flutter_rust_bridge::frb(init)]
pub fn init_app() {
    // Default utilities - feel free to customize
    // flutter_rust_bridge::setup_default_user_utils();
    env_logger::init();
}
