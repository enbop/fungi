use libp2p_identity::PeerId;
use libp2p_stream::Control;
use libp2p_swarm::StreamProtocol;
use tokio::net::ToSocketAddrs;

use crate::tcp_tunneling::copy_stream;

pub async fn forward_port_to_peer<A: ToSocketAddrs>(
    mut stream_control: Control,
    local_addr: A,
    target_peer: PeerId,
    target_protocol: StreamProtocol,
) {
    let listener = tokio::net::TcpListener::bind(local_addr).await.unwrap();
    log::info!(
        "Listening on {} for TCP tunneling",
        listener.local_addr().unwrap()
    );
    loop {
        let (tcp_stream, _) = listener.accept().await.unwrap();
        let libp2p_stream = stream_control
            .open_stream(target_peer, target_protocol.clone())
            .await
            .unwrap();
        tokio::spawn(async move {
            println!(
                "new sub stream from {:?} to {:?}",
                tcp_stream.peer_addr().unwrap(),
                target_peer
            );
            copy_stream(libp2p_stream, tcp_stream).await;
            println!("sub stream closed");
        });
    }
}
