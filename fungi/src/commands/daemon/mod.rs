mod daemon;
pub mod listeners;
use super::FungiArgs;
use crate::config::FungiConfig;

pub async fn daemon(args: FungiArgs, config: &FungiConfig) {
    println!("Starting Fungi daemon...");
    let fungi_dir = args.fungi_dir();
    println!("Fungi directory: {:?}", fungi_dir);
    let mut daemon = daemon::FungiDaemon::new(args, config.clone()).await;
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
