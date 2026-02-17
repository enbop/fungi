use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::{Context as _, bail};

use libp2p::{
    PeerId, Stream, StreamProtocol,
    futures::{AsyncReadExt as _, AsyncWriteExt as _, StreamExt, channel::oneshot},
    swarm::ConnectionId,
};
use parking_lot::Mutex;
use tokio::sync::mpsc;

// https://github.com/libp2p/specs/blob/master/ping/ping.md#protocol
const PING_PROTOCOL: StreamProtocol = StreamProtocol::new("/ipfs/ping/1.0.0");

pub struct PingState {
    stream_control: Option<libp2p_stream::Control>,
    outbound_interval: Duration,
    outbound: Arc<Mutex<HashMap<ConnectionId, OutboundPingState>>>,
    event_tx: mpsc::UnboundedSender<PingRttEvent>,
}

#[derive(Debug, Clone)]
pub struct PingRttEvent {
    pub peer_id: PeerId,
    pub connection_id: ConnectionId,
    pub rtt: Duration,
}

impl PingState {
    pub fn new(outbound_interval: Duration, event_tx: mpsc::UnboundedSender<PingRttEvent>) -> Self {
        Self {
            outbound_interval,
            stream_control: None,
            outbound: Arc::new(Mutex::new(HashMap::new())),
            event_tx,
        }
    }

    pub fn init(&mut self, mut stream_control: libp2p_stream::Control) {
        assert!(
            self.stream_control.is_none(),
            "PingState already initialized"
        );
        let incoming = stream_control
            .accept(PING_PROTOCOL)
            .expect("Listening on ping protocol should be only once");
        start_pong_loop(incoming);
        self.stream_control = Some(stream_control);
    }

    pub fn start_outbound_ping(&self, peer_id: PeerId, connection_id: ConnectionId) {
        assert!(
            self.stream_control.is_some(),
            "PingState not initialized with stream control"
        );
        let mut lock = self.outbound.lock();
        if let Some(state) = lock.get(&connection_id) {
            if !state.is_finished() {
                // previous ping task is still running
                log::debug!("Outbound ping to {} is already running", peer_id);
                return;
            }
        }
        lock.insert(
            connection_id,
            OutboundPingState::new(
                peer_id,
                connection_id,
                self.stream_control.as_ref().unwrap().clone(),
                self.outbound_interval,
                self.event_tx.clone(),
            ),
        );
    }

    pub fn stop_outbound_ping(&self, connection_id: ConnectionId) {
        self.outbound.lock().remove(&connection_id);
    }

    pub async fn ping_now(&self, connection_id: ConnectionId) -> anyhow::Result<Duration> {
        let ping_trigger = {
            let lock = self.outbound.lock();
            lock.get(&connection_id)
                .map(|state| state.ping_trigger.clone())
                .ok_or_else(|| {
                    anyhow::anyhow!("No outbound ping task found for connection {connection_id:?}")
                })?
        };

        let (tx, rx) = oneshot::channel::<Duration>();
        ping_trigger
            .send(tx)
            .await
            .with_context(|| format!("Failed to trigger ping for connection {connection_id:?}"))?;
        rx.await.with_context(|| {
            format!("Failed to receive ping result for connection {connection_id:?}")
        })
    }
}

struct OutboundPingState {
    // Sender to trigger an immediate ping
    // Returns RTT result through the oneshot channel
    ping_trigger: mpsc::Sender<oneshot::Sender<Duration>>,
    task_alive: Arc<()>,
}

impl OutboundPingState {
    fn new(
        peer_id: PeerId,
        connection_id: ConnectionId,
        mut stream_control: libp2p_stream::Control,
        interval: Duration,
        event_tx: mpsc::UnboundedSender<PingRttEvent>,
    ) -> Self {
        let task_alive = Arc::new(());
        let task_alive_guard = task_alive.clone();

        let (ping_trigger, mut ping_trigger_receiver) =
            mpsc::channel::<oneshot::Sender<Duration>>(8);
        tokio::spawn(async move {
            log::debug!(
                "Starting outbound ping to {}, connection {:?}",
                peer_id,
                connection_id
            );

            let _task_alive_guard = task_alive_guard;

            if let Err(e) = run_outbound_ping_task(
                peer_id,
                connection_id,
                &mut stream_control,
                interval,
                &mut ping_trigger_receiver,
                event_tx,
            )
            .await
            {
                log::warn!("{:#}", e);
            }

            log::debug!(
                "Outbound ping task ended for {}, connection {:?}",
                peer_id,
                connection_id
            );
        });

        Self {
            ping_trigger,
            task_alive,
        }
    }

    fn is_finished(&self) -> bool {
        Arc::strong_count(&self.task_alive) == 1
    }
}

async fn run_outbound_ping_task(
    peer_id: PeerId,
    connection_id: ConnectionId,
    stream_control: &mut libp2p_stream::Control,
    interval: Duration,
    ping_trigger_receiver: &mut mpsc::Receiver<oneshot::Sender<Duration>>,
    event_tx: mpsc::UnboundedSender<PingRttEvent>,
) -> anyhow::Result<()> {
    let mut stream = stream_control
        .open_stream_on_connection(peer_id, connection_id, PING_PROTOCOL)
        .await
        .with_context(|| format!("Failed to open ping stream to {}", peer_id))?;
    stream.ignore_for_keep_alive();

    loop {
        let callback = match tokio::time::timeout(interval, ping_trigger_receiver.recv()).await {
            Ok(Some(cb)) => Some(cb),
            Ok(None) => {
                bail!("Ping trigger channel closed for connection {connection_id:?}");
            }
            Err(_) => None, // interval ticked
        };

        let rtt = send_ping(&mut stream)
            .await
            .with_context(|| format!("Ping to {} failed", peer_id))?;
        let _ = event_tx.send(PingRttEvent {
            peer_id,
            connection_id,
            rtt,
        });
        if let Some(callback) = callback {
            let _ = callback.send(rtt);
        }
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

fn start_pong_loop(mut incoming: libp2p_stream::IncomingStreams) {
    tokio::spawn(async move {
        log::debug!("Ping pong loop started");
        loop {
            // TODO check connection count limit for each peer
            // https://github.com/libp2p/specs/blob/master/ping/ping.md#protocol
            let Some((peer_id, mut stream)) = incoming.next().await else {
                log::error!("Ping incoming stream ended unexpectedly");
                break;
            };
            stream.ignore_for_keep_alive();
            log::debug!("Received ping stream from {}", peer_id);
            tokio::spawn(async move {
                loop {
                    // The dialing peer sends a 32-byte payload of random binary data on an open stream.
                    // The listening peer echoes the same 32-byte payload back to the dialing peer.
                    // The dialing peer then measures the RTT from when it wrote the bytes to when it received them.
                    let mut buf = [0u8; 32];
                    if let Err(e) = stream.read_exact(&mut buf).await {
                        log::warn!("Failed to read ping payload: {}", e);
                        break;
                    }
                    if let Err(e) = stream.write_all(&buf).await {
                        log::warn!("Failed to write ping payload: {}", e);
                        break;
                    }
                    log::trace!("Ponged ping from {}", peer_id);
                }
                log::info!("Ping pong loop ended for a stream from {}", peer_id);
            });
        }
    });
}
