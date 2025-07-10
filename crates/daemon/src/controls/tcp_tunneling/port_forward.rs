use fungi_swarm::SwarmControl;
use libp2p::{PeerId, StreamProtocol};
use libp2p_stream::Control;
use std::net::SocketAddr;
use tokio_util::compat::FuturesAsyncReadCompatExt;

// TODO remove unwraps handle errors properly
pub async fn forward_port_to_peer(
    swarm_control: SwarmControl,
    mut stream_control: Control,
    local_addr: SocketAddr,
    target_peer: PeerId,
    target_protocol: StreamProtocol,
) {
    let listener = tokio::net::TcpListener::bind(local_addr).await.unwrap();
    log::info!(
        "Listening on {} for TCP tunneling",
        listener.local_addr().unwrap()
    );
    loop {
        let (mut tcp_stream, _) = listener.accept().await.unwrap();
        // TODO: DialError::Aborted at first connecting.
        if let Err(e) = swarm_control.connect(target_peer).await {
            log::error!("Failed to connect to peer {}: {}", target_peer, e);
            continue;
        }
        let Ok(libp2p_stream) = stream_control
            .open_stream(target_peer, target_protocol.clone())
            .await
        else {
            log::error!("Failed to open stream to peer {}", target_peer);
            continue;
        };
        tokio::spawn(async move {
            println!(
                "new sub stream from {:?} to {:?}",
                tcp_stream.peer_addr().unwrap(),
                target_peer
            );
            tokio::io::copy_bidirectional(&mut libp2p_stream.compat(), &mut tcp_stream)
                .await
                .ok();
            println!("sub stream closed");
        });
    }
}
