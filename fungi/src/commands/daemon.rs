use fungi_gateway::{SwarmState, SwarmWrapper};
use fungi_util::tcp_tunneling;
use home::home_dir;
use libp2p::StreamProtocol;
use std::time::Duration;

pub async fn daemon() {
    println!("Starting Fungi daemon...");
    let fungi_dir = home_dir().unwrap().join(".fungi");
    let swarm = SwarmState::start_libp2p_swarm(&fungi_dir).await.unwrap();
    let peer_id = swarm.local_peer_id();
    println!("Local Peer ID: {:?}", peer_id);

    apply_tcp_tunneling(swarm.clone()).await;

    loop {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(20)) => {
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

async fn apply_tcp_tunneling(mut swarm: SwarmWrapper) {
    // test tcp port forwarding, forward local port 9001 to ${peerId} with libp2p protocol /tunnel-test
    // swarm.add_peer_addresses(peer_id, addrs)
    let target_peer = todo!();
    let target_protocol = StreamProtocol::new("/tunnel-test");
    let stream_control = swarm.new_stream_control().await;
    tokio::spawn(tcp_tunneling::forward_port_to_peer(
        stream_control.clone(),
        format!("127.0.0.1:9001"),
        target_peer,
        target_protocol.clone(),
    ));

    // test tcp port listen, listen on libp2p protocol /tunnel-test to local port 9002
    tokio::spawn(tcp_tunneling::listen_p2p_to_port(
        stream_control,
        target_protocol,
        format!("127.0.0.1:9002").parse().unwrap(),
    ));
}
