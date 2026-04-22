use std::time::Duration;

use crate::SwarmControl;
use anyhow::Context as _;
use fungi_util::protocols::FUNGI_PROBE_PROTOCOL;

use libp2p::{
    Stream,
    futures::{AsyncReadExt as _, AsyncWriteExt as _, StreamExt},
    swarm::ConnectionId,
};

pub(crate) async fn ping_connection(
    mut stream_control: fungi_stream::Control,
    connection_id: ConnectionId,
    timeout: Duration,
) -> anyhow::Result<Duration> {
    let mut stream = stream_control
        .open_stream_by_id(connection_id, FUNGI_PROBE_PROTOCOL)
        .await
        .with_context(|| {
            format!(
                "Failed to open probe stream to connection {}",
                connection_id
            )
        })?;
    stream.ignore_for_keep_alive();

    tokio::time::timeout(timeout, send_ping(&mut stream))
        .await
        .context("Ping timed out")
        .and_then(|res| res.context("Ping failed"))
}

async fn send_ping(stream: &mut Stream) -> Result<Duration, std::io::Error> {
    let payload = [42u8; 32];
    let mut resp = [0u8; 32];
    let start = tokio::time::Instant::now();
    stream.write_all(&payload).await?;
    stream.flush().await?;
    stream.read_exact(&mut resp).await?;
    if resp != payload {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Pong payload mismatch",
        ));
    }
    Ok(start.elapsed())
}

pub(crate) async fn probe_pong_loop(swarm_control: SwarmControl) {
    let mut incoming = swarm_control
        .accept_incoming_streams(FUNGI_PROBE_PROTOCOL)
        .expect("Listening on probe protocol should be only once");
    log::debug!("Probe pong loop started");
    loop {
        let Some(incoming_stream) = incoming.next().await else {
            log::error!("Ping incoming stream ended unexpectedly");
            break;
        };
        let peer_id = incoming_stream.peer_id;
        let connection_id = incoming_stream.connection_id;
        let mut stream = incoming_stream.stream;
        stream.ignore_for_keep_alive();
        log::debug!(
            "Received probe stream from {} on connection {:?}",
            peer_id,
            connection_id
        );
        tokio::spawn(async move {
            loop {
                // The dialing peer sends a 32-byte payload of random binary data on an open stream.
                // The listening peer echoes the same 32-byte payload back to the dialing peer.
                // The dialing peer then measures the RTT from when it wrote the bytes to when it received them.
                let mut buf = [0u8; 32];
                if (stream.read_exact(&mut buf).await).is_err() {
                    break;
                }
                if (stream.write_all(&buf).await).is_err() {
                    break;
                }
                log::trace!("Ponged ping from {}", peer_id);
            }
        });
    }
}
