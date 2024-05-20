use fungi_gateway::SwarmState;
use futures::{AsyncReadExt, AsyncWriteExt, StreamExt as _};
use libp2p::StreamProtocol;
use libp2p_stream::IncomingStreams;
use std::time::Duration;

pub async fn daemon() {
    println!("Starting Fungi daemon...");
    let mut swarm = SwarmState::start_libp2p_swarm().await.unwrap();
    let peer_id = swarm.local_peer_id();
    log::debug!("Peer ID: {:?}", peer_id);

    let test_stream_listener = swarm
        .stream_accept(StreamProtocol::new("/echo"))
        .await
        .unwrap();
    tokio::spawn(start_test_stream_listener(test_stream_listener));

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

async fn start_test_stream_listener(mut listener: IncomingStreams) {
    log::debug!("Starting test stream listener...");
    loop {
        let Some((peer_id, stream)) = listener.next().await else {
            break;
        };
        log::debug!("Received test stream from {:?}", peer_id);
        tokio::spawn(handle_test_stream(stream));
    }
    log::debug!("Stream listener closed");
}

async fn handle_test_stream(stream: libp2p::Stream) {
    let (mut reader, mut writer) = stream.split();
    let mut buf = [0u8; 1024];
    loop {
        match reader.read(&mut buf).await {
            Ok(0) => {
                log::debug!("Stream closed");
                break;
            }
            Ok(n) => {
                log::debug!("Received {} bytes", n);
                writer.write_all(&buf[..n]).await.unwrap();
            }
            Err(e) => {
                log::error!("Error reading stream: {:?}", e);
                break;
            }
        }
    }
}
