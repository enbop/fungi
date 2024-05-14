use std::time::Duration;

use fungi_gateway::{start_libp2p_swarm, AsyncSwarmRequest};

pub async fn daemon() {
    println!("Starting Fungi daemon...");
    tokio::spawn(start_libp2p_swarm());

    loop {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(2)) => {
                let peer_id = AsyncSwarmRequest::get_peer_id().await;
                log::debug!("Peer ID: {:?}", peer_id);
            }
            _ = tokio::signal::ctrl_c() => {
                println!("Shutting down Fungi daemon...");
                break;
            }
        }        
    }
}