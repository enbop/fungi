mod daemon;
mod listeners;

use std::time::Duration;

use crate::config::FungiConfig;

use super::FungiArgs;

pub async fn daemon(args: &FungiArgs, config: &FungiConfig) {
    println!("Starting Fungi daemon...");
    let fungi_dir = args.fungi_dir();
    println!("Fungi directory: {:?}", fungi_dir);
    let mut daemon = daemon::FungiDaemon::new(fungi_dir.clone(), config.clone()).await;
    daemon.start();

    let swarm_wrapper = {
        let lock = daemon.swarm_state.lock().unwrap();
        println!("Local Peer ID: {}", lock.local_peer_id());
        lock.clone()
    };

    loop {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(5)) => {
                let info = swarm_wrapper.network_info().await;
                log::info!("Network info: {:?}", info);
            }
            _ = tokio::signal::ctrl_c() => {
                println!("Shutting down Fungi daemon...");
                break;
            }
        }
    }
}
