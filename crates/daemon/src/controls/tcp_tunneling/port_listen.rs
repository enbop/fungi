use futures::StreamExt;
use libp2p::Stream;
use libp2p_stream::IncomingStreams;
use parking_lot::Mutex;
use std::net::SocketAddr;
use std::sync::Arc;
use thiserror::Error;
use tokio::task::JoinHandle;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tokio_util::sync::CancellationToken;

#[derive(Error, Debug)]
pub enum TcpTunnelingError {
    #[error("Failed to connect to target address {addr}: {source}")]
    TcpConnect {
        addr: SocketAddr,
        #[source]
        source: std::io::Error,
    },
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

type Result<T> = std::result::Result<T, TcpTunnelingError>;

pub async fn listen_p2p_to_port(
    mut incomings: IncomingStreams,
    target_addr: SocketAddr,
    cancellation_token: CancellationToken,
) -> Result<()> {
    // Store active connection tasks for graceful shutdown
    let active_tasks: Arc<Mutex<Vec<JoinHandle<()>>>> = Arc::new(Mutex::new(Vec::new()));
    let active_tasks_for_cleanup = active_tasks.clone();

    loop {
        tokio::select! {
            stream_result = incomings.next() => {
                match stream_result {
                    Some((peer_id, stream)) => {
                        log::debug!("Received stream from {peer_id:?}");

                        let task = tokio::spawn(handle_incoming_stream(stream, target_addr));
                        active_tasks.lock().push(task);

                        // Clean up completed tasks
                        active_tasks.lock().retain(|task| !task.is_finished());
                    }
                    None => {
                        log::debug!("Stream listener closed");
                        break;
                    }
                }
            }
            _ = cancellation_token.cancelled() => {
                log::info!("Received cancellation signal, shutting down P2P to port listener");
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

    log::debug!("P2P to port listener stopped gracefully");
    Ok(())
}

async fn handle_incoming_stream(p2p_stream: Stream, target_addr: SocketAddr) {
    match handle_incoming_stream_inner(p2p_stream, target_addr).await {
        Ok(()) => log::debug!("Connection to {target_addr} closed successfully"),
        Err(e) => log::error!("Connection to {target_addr} failed: {e}"),
    }
}

async fn handle_incoming_stream_inner(p2p_stream: Stream, target_addr: SocketAddr) -> Result<()> {
    let mut target_stream =
        tokio::net::TcpStream::connect(target_addr)
            .await
            .map_err(|source| TcpTunnelingError::TcpConnect {
                addr: target_addr,
                source,
            })?;

    log::debug!("Established connection to {target_addr}");

    tokio::io::copy_bidirectional(&mut p2p_stream.compat(), &mut target_stream)
        .await
        .map_err(TcpTunnelingError::Io)?;

    log::debug!("Connection to {target_addr} closed");
    Ok(())
}
