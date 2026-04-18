use std::time::Duration;

use anyhow::Context as _;
use fungi_util::protocols::FUNGI_PROBE_PROTOCOL;

use libp2p::{
    PeerId, Stream,
    futures::{AsyncReadExt as _, AsyncWriteExt as _, StreamExt},
    swarm::ConnectionId,
};

// Keep the standard libp2p ping behaviour free to own /ipfs/ping/1.0.0. This stream-based
// variant is reserved for fungi's explicit connection probes.

pub struct PingState {
    stream_control: Option<fungi_stream::Control>,
}

impl PingState {
    pub fn new() -> Self {
        Self {
            stream_control: None,
        }
    }

    pub fn init(&mut self, mut stream_control: fungi_stream::Control) {
        assert!(
            self.stream_control.is_none(),
            "PingState already initialized"
        );
        let incoming = stream_control
            .listen(FUNGI_PROBE_PROTOCOL)
            .expect("Listening on probe protocol should be only once");
        start_pong_loop(incoming);
        self.stream_control = Some(stream_control);
    }

    pub async fn ping_now(
        &self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        timeout: Duration,
    ) -> anyhow::Result<Duration> {
        let mut stream_control = self
            .stream_control
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("PingState not initialized with stream control"))?
            .clone();

        let mut stream = stream_control
            .open_stream(connection_id, FUNGI_PROBE_PROTOCOL)
            .await
            .with_context(|| format!("Failed to open probe stream to {}", peer_id))?;
        stream.ignore_for_keep_alive();

        send_ping_with_timeout(&mut stream, peer_id, timeout).await
    }
}

pub(crate) async fn send_ping_with_timeout(
    stream: &mut Stream,
    peer_id: PeerId,
    timeout: Duration,
) -> anyhow::Result<Duration> {
    match tokio::time::timeout(timeout, send_ping(stream)).await {
        Ok(Ok(rtt)) => Ok(rtt),
        Ok(Err(e)) => Err(anyhow::Error::new(e).context(format!("Ping to {} failed", peer_id))),
        Err(_) => anyhow::bail!(
            "Ping to {} timed out after {}ms",
            peer_id,
            timeout.as_millis()
        ),
    }
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

fn start_pong_loop(mut incoming: fungi_stream::IncomingStreams) {
    tokio::spawn(async move {
        log::debug!("Probe pong loop started");
        loop {
            // TODO check connection count limit for each peer
            // https://github.com/libp2p/specs/blob/master/ping/ping.md#protocol
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
    });
}
