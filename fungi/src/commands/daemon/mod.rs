mod daemon;
pub mod listeners;

use std::time::Duration;

use crate::config::FungiConfig;

use super::FungiArgs;

pub async fn daemon(args: &FungiArgs, config: &FungiConfig) {
    println!("Starting Fungi daemon...");
    let fungi_dir = args.fungi_dir();
    println!("Fungi directory: {:?}", fungi_dir);
    let mut daemon = daemon::FungiDaemon::new(fungi_dir.clone(), config.clone()).await;
    daemon.start().await;

    println!("Local Peer ID: {}", daemon.swarm_state.local_peer_id());

    loop {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(5)) => {
                let info = daemon.swarm_state.network_info().await;
                log::info!("Network info: {:?}", info);
            }
            _ = tokio::signal::ctrl_c() => {
                println!("Shutting down Fungi daemon...");
                break;
            }
        }
    }
}
