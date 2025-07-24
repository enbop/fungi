use fungi_swarm::SwarmControl;
use libp2p::{PeerId, StreamProtocol};
use libp2p_stream::Control;
use parking_lot::Mutex;
use std::net::SocketAddr;
use std::sync::Arc;
use thiserror::Error;
use tokio::task::JoinHandle;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tokio_util::sync::CancellationToken;

#[derive(Error, Debug)]
pub enum PortForwardError {
    #[error("Failed to bind to local address {addr}: {source}")]
    BindLocal {
        addr: SocketAddr,
        #[source]
        source: std::io::Error,
    },
    #[error("Failed to accept TCP connection: {0}")]
    AcceptTcp(#[from] std::io::Error),
    #[error("Failed to connect to peer {peer}: {source}")]
    ConnectPeer {
        peer: PeerId,
        #[source]
        source: anyhow::Error,
    },
    #[error("Failed to open stream to peer {0}")]
    OpenStream(PeerId, #[source] libp2p_stream::OpenStreamError),
}

type Result<T> = std::result::Result<T, PortForwardError>;

pub async fn forward_port_to_peer(
    swarm_control: SwarmControl,
    stream_control: Control,
    local_addr: SocketAddr,
    target_peer: PeerId,
    target_protocol: StreamProtocol,
    cancellation_token: CancellationToken,
) -> Result<()> {
    let listener = tokio::net::TcpListener::bind(local_addr)
        .await
        .map_err(|source| PortForwardError::BindLocal {
            addr: local_addr,
            source,
        })?;

    let actual_addr = listener
        .local_addr()
        .map_err(|source| PortForwardError::BindLocal {
            addr: local_addr,
            source,
        })?;

    log::info!("Listening on {actual_addr} for TCP tunneling");

    // Store active connection tasks for graceful shutdown
    let active_tasks: Arc<Mutex<Vec<JoinHandle<()>>>> = Arc::new(Mutex::new(Vec::new()));
    let active_tasks_for_cleanup = active_tasks.clone();

    loop {
        tokio::select! {
            accept_result = listener.accept() => {
                match accept_result {
                    Ok((tcp_stream, client_addr)) => {
                        log::debug!("Accepted connection from {client_addr}");

                        let swarm_control = swarm_control.clone();
                        let stream_control = stream_control.clone();
                        let target_protocol = target_protocol.clone();

                        let task = tokio::spawn(async move {
                            if let Err(e) = handle_tcp_connection(
                                swarm_control,
                                stream_control,
                                tcp_stream,
                                target_peer,
                                target_protocol,
                            ).await {
                                log::error!("Failed to handle connection from {client_addr}: {e}");
                            }
                        });

                        active_tasks.lock().push(task);

                        // Clean up completed tasks
                        active_tasks.lock().retain(|task| !task.is_finished());
                    }
                    Err(e) => {
                        log::error!("Failed to accept connection: {e}");
                        continue;
                    }
                }
            }
            _ = cancellation_token.cancelled() => {
                log::info!("Received cancellation signal, shutting down port forwarder");
                break;
            }
        }
    }

    // Cancel all active tasks
    let tasks = std::mem::take(&mut *active_tasks_for_cleanup.lock());
    for task in tasks {
        task.abort();
        let _ = task.await;
    }

    log::debug!("Port forwarder stopped gracefully");
    Ok(())
}

async fn handle_tcp_connection(
    swarm_control: SwarmControl,
    mut stream_control: Control,
    mut tcp_stream: tokio::net::TcpStream,
    target_peer: PeerId,
    target_protocol: StreamProtocol,
) -> Result<()> {
    // Connect to peer
    swarm_control
        .connect(target_peer)
        .await
        .map_err(|source| PortForwardError::ConnectPeer {
            peer: target_peer,
            source: source.into(),
        })?;

    // Open stream to peer
    let libp2p_stream = stream_control
        .open_stream(target_peer, target_protocol)
        .await
        .map_err(|e| PortForwardError::OpenStream(target_peer, e))?;

    let tcp_peer_addr = tcp_stream
        .peer_addr()
        .map_err(PortForwardError::AcceptTcp)?;
    log::debug!("Established tunnel from {tcp_peer_addr} to peer {target_peer}");

    // Bidirectional copy
    tokio::io::copy_bidirectional(&mut libp2p_stream.compat(), &mut tcp_stream)
        .await
        .map_err(PortForwardError::AcceptTcp)?;

    log::debug!("Tunnel from {tcp_peer_addr} to peer {target_peer} closed");
    Ok(())
}
