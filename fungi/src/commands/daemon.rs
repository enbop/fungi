use std::time::Duration;
use fungi_gateway::SwarmState;

pub async fn daemon() {
    println!("Starting Fungi daemon...");
    let swarm = SwarmState::start_libp2p_swarm().await.unwrap();

    loop {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(2)) => {
                let peer_id = swarm.get_peer_id().await;
                log::debug!("Peer ID: {:?}", peer_id);
            }
            _ = tokio::signal::ctrl_c() => {
                println!("Shutting down Fungi daemon...");
                break;
            }
        }
    }
}