mod daemon;
pub mod listeners;
use super::DaemonArgs;
use tokio::sync::OnceCell;

static ALL_IN_ONE_BINARY: OnceCell<bool> = OnceCell::const_new();

pub async fn daemon(args: DaemonArgs, all_in_one_binary: bool) {
    ALL_IN_ONE_BINARY.set(all_in_one_binary).unwrap();
    crate::commands::init(&args).unwrap();

    println!("Starting Fungi daemon...");
    let mut daemon = daemon::FungiDaemon::new(args).await;
    daemon.start().await;

    println!("Local Peer ID: {}", daemon.swarm_daemon.local_peer_id());

    let network_info = daemon
        .swarm_daemon
        .invoke_swarm(|swarm| swarm.network_info())
        .await
        .unwrap();
    println!("Network info: {:?}", network_info);

    tokio::signal::ctrl_c().await.ok();
    println!("Shutting down Fungi daemon...");
}
