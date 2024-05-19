use std::time::Duration;
use fungi_gateway::SwarmState;

pub async fn daemon() {
    println!("Starting Fungi daemon...");
    let swarm = SwarmState::start_libp2p_swarm().await.unwrap();
    let peer_id = swarm.local_peer_id();
    log::debug!("Peer ID: {:?}", peer_id);

    loop {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(2)) => {
                let info = swarm.network_info().await;
                log::debug!("Network info: {:?}", info);
            }
            _ = tokio::signal::ctrl_c() => {
                println!("Shutting down Fungi daemon...");
                break;
            }
        }
    }
}