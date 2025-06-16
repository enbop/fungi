use std::sync::Mutex;

use fungi_daemon::FungiDaemon;
use once_cell::sync::Lazy;

static FUNGI_DAEMON: Lazy<Mutex<Option<FungiDaemon>>> = Lazy::new(|| Default::default());

pub async fn start_fungi_daemon() -> anyhow::Result<()> {
    if FUNGI_DAEMON.lock().unwrap().is_some() {
        log::warn!("Fungi daemon is already running.");
        return Ok(());
    }

    let args = fungi_daemon::DaemonArgs::default();
    fungi_config::init(&args).unwrap();

    let daemon = fungi_daemon::FungiDaemon::start(args).await?;
    log::info!(
        "Fungi local peer id: {:?}",
        daemon.swarm_control.local_peer_id()
    );

    *FUNGI_DAEMON.lock().unwrap() = Some(daemon);
    Ok(())
}

pub async fn peer_id() -> Option<String> {
    FUNGI_DAEMON
        .lock()
        .unwrap()
        .as_ref()
        .map(|daemon| daemon.swarm_control.local_peer_id().to_string())
}

#[flutter_rust_bridge::frb(init)]
pub fn init_app() {
    // Default utilities - feel free to customize
    // flutter_rust_bridge::setup_default_user_utils();
    env_logger::init();
}
