use std::net::SocketAddr;

use futures::StreamExt;
use libp2p_stream::Control;
use libp2p_swarm::{Stream, StreamProtocol};

use crate::copy_stream;

pub async fn listen_p2p_to_port(
    mut stream_control: Control,
    target_protocol: StreamProtocol,
    target_addr: SocketAddr,
) {
    let mut incomings = stream_control.accept(target_protocol).unwrap();
    loop {
        let Some((peer_id, stream)) = incomings.next().await else {
            break;
        };
        log::debug!("Received test stream from {:?}", peer_id);
        tokio::spawn(handle_income(stream, target_addr));
    }
    log::debug!("Stream listener closed");
}

async fn handle_income(p2p_stream: Stream, target_addr: SocketAddr) {
    let target_stream = tokio::net::TcpStream::connect(target_addr).await.unwrap();
    tokio::spawn(async move {
        println!("new sub stream to {:?}", target_addr);
        copy_stream(p2p_stream, target_stream).await;
        println!("sub stream closed");
    });
}
