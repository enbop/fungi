use super::{
    ConnectionRecordSliceExt, ConnectionSelectionStrategy, TSwarm,
    dial_plan::DialPlan,
    relay::{RefreshThrottle, RelayPeers},
};
use crate::{ConnectionRecord, State, StreamObservationHandle, ping};
use anyhow::{Result, bail};
use async_result::{AsyncResult, Completer};
use libp2p::{
    Multiaddr, PeerId, Stream, StreamProtocol,
    swarm::{
        ConnectionId, DialError,
        dial_opts::{DialOpts, PeerCondition},
    },
};
use parking_lot::Mutex;
use std::{
    any::Any,
    collections::HashMap,
    ops::Deref,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use thiserror::Error;
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard, mpsc::UnboundedSender};

#[derive(Error, Debug)]
pub enum ConnectError {
    #[error("Dial failed: {0}")]
    DialFailed(#[from] DialError),
    #[error("Swarm invocation failed: {0}")]
    SwarmInvocationFailed(anyhow::Error),
    #[error("Connection cancelled")]
    Cancelled,
    #[error("Dial to peer {peer_id} timed out")]
    DialTimeout { peer_id: PeerId },
    #[error("No dial addresses available for peer {peer_id}")]
    NoDialAddresses { peer_id: PeerId },
}

const CONNECT_TIMEOUT: Duration = Duration::from_secs(6);
const DIRECT_DIAL_TIMEOUT: Duration = Duration::from_secs(3);

type SwarmResponse = Box<dyn Any + Send>;
type SwarmRequest = Box<dyn FnOnce(&mut TSwarm) -> SwarmResponse + Send + Sync>;

pub struct SwarmAsyncCall {
    pub(super) request: SwarmRequest,
    pub(super) response: Completer<SwarmResponse>,
}

impl SwarmAsyncCall {
    pub(super) fn new(request: SwarmRequest, response: Completer<SwarmResponse>) -> Self {
        Self { request, response }
    }
}

#[derive(Clone)]
pub struct SwarmControl {
    local_peer_id: Arc<PeerId>,
    swarm_caller_tx: UnboundedSender<SwarmAsyncCall>,
    stream_control: fungi_stream::Control,
    connection_selection_strategy: ConnectionSelectionStrategy,
    connect_locks: Arc<Mutex<HashMap<PeerId, Arc<AsyncMutex<()>>>>>,
    pub(super) refresh_throttle: RefreshThrottle,
    pub(super) relay_peers: RelayPeers,
    state: State,
}

impl Deref for SwarmControl {
    type Target = UnboundedSender<SwarmAsyncCall>;

    fn deref(&self) -> &Self::Target {
        &self.swarm_caller_tx
    }
}

impl SwarmControl {
    pub(super) fn new(
        local_peer_id: Arc<PeerId>,
        swarm_caller_tx: UnboundedSender<SwarmAsyncCall>,
        stream_control: fungi_stream::Control,
        refresh_throttle: RefreshThrottle,
        relay_peers: RelayPeers,
        state: State,
    ) -> Self {
        relay_peers.register_with_state(&state);
        Self {
            local_peer_id,
            swarm_caller_tx,
            stream_control,
            connection_selection_strategy: ConnectionSelectionStrategy::default(),
            connect_locks: Arc::new(Mutex::new(HashMap::new())),
            refresh_throttle,
            relay_peers,
            state,
        }
    }

    pub fn local_peer_id(&self) -> PeerId {
        *self.local_peer_id
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    pub(super) fn connection_selection_strategy(&self) -> ConnectionSelectionStrategy {
        self.connection_selection_strategy
    }

    pub fn accept_incoming_streams(
        &self,
        protocol: StreamProtocol,
    ) -> std::result::Result<fungi_stream::IncomingStreams, fungi_stream::AlreadyRegistered> {
        let mut stream_control = self.stream_control.clone();
        stream_control.listen(protocol)
    }

    pub async fn ping_connection(
        &self,
        connection_id: ConnectionId,
        timeout: Duration,
    ) -> Result<Duration> {
        let stream_control = self.stream_control.clone();
        let rtt = ping::ping_connection(stream_control, connection_id, timeout).await?;
        self.state.update_connection_ping(&connection_id, rtt);
        Ok(rtt)
    }

    pub async fn close_connection(&self, connection_id: ConnectionId) -> Result<bool> {
        self.invoke_swarm(move |swarm| swarm.close_connection(connection_id))
            .await
    }

    pub async fn open_stream(
        &self,
        peer_id: PeerId,
        protocol: StreamProtocol,
    ) -> Result<(Stream, StreamObservationHandle, ConnectionId)> {
        let connections = self.connect(peer_id).await?;

        if connections.is_empty() {
            bail!(
                "No connections available to peer {} for opening stream with protocol",
                peer_id,
            );
        }

        let first = &connections[0];
        let mut stream_control = self.stream_control.clone();
        let stream = stream_control
            .open_stream_by_id(first.connection_id, protocol.clone())
            .await?;
        return Ok((
            stream,
            self.state
                .track_outbound_stream_opened(peer_id, first.connection_id, protocol),
            first.connection_id,
        ));
    }

    pub async fn connect(&self, peer_id: PeerId) -> Result<Vec<ConnectionRecord>, ConnectError> {
        // Stopgap singleflight: keep at most one live connect attempt per peer so
        // a burst of open-stream requests does not fan out into duplicate dials.
        let _connect_guard = self.acquire_connect_lock(peer_id).await?;

        let mut connections = self.state.get_connections_by_peer_id(&peer_id);
        connections.sort_by_strategy(self.connection_selection_strategy);

        if !connections.is_empty() {
            return Ok(connections);
        }

        // Dial direct addresses
        let dial_plan = DialPlan::for_peer(&self.state, peer_id);
        let direct_addresses = dial_plan.direct_addresses();

        let has_relay_peers = !self.relay_peers.is_empty();
        let mut relay_task_handle = None;
        let relay_submit_failed = Arc::new(AtomicBool::new(false));

        if direct_addresses.is_empty() {
            // No direct dial candidates, fallback to relay if available
            if has_relay_peers {
                // Immediately dial relay
                relay_task_handle = Some(self.spawn_relay_dial_task(
                    peer_id,
                    Duration::ZERO,
                    relay_submit_failed.clone(),
                ));
            } else {
                log::info!(
                    "No stored direct dial candidates or relay peers available for peer {}",
                    peer_id,
                );
                return Err(ConnectError::NoDialAddresses { peer_id });
            }

            self.wait_for_state(CONNECT_TIMEOUT, |state| {
                state.connection_len_for_peer(&peer_id) > 0
                    || relay_submit_failed.load(Ordering::Relaxed)
            })
            .await;
        } else {
            // Start with direct dial, but if relay peers are available, also start dialing relay in parallel after a short delay
            let direct_submit_failed = self
                .dial_with_addresses(peer_id, direct_addresses, PeerCondition::Always)
                .await
                .is_err();

            if direct_submit_failed && !has_relay_peers {
                return Ok(vec![]);
            }

            if has_relay_peers {
                let relay_delay = if direct_submit_failed {
                    Duration::ZERO
                } else {
                    DIRECT_DIAL_TIMEOUT
                };
                relay_task_handle = Some(self.spawn_relay_dial_task(
                    peer_id,
                    relay_delay,
                    relay_submit_failed.clone(),
                ));
            }

            self.wait_for_state(CONNECT_TIMEOUT, |state| {
                if direct_submit_failed {
                    state.connection_len_for_peer(&peer_id) > 0
                        || relay_submit_failed.load(Ordering::Relaxed)
                } else {
                    state
                        .get_connections_by_peer_id(&peer_id)
                        .iter()
                        .any(|c| !c.is_relay())
                        || (relay_submit_failed.load(Ordering::Relaxed) && !has_relay_peers)
                }
            })
            .await;
        };

        if let Some(handle) = relay_task_handle {
            handle.abort();
        }

        let mut connections = self.state.get_connections_by_peer_id(&peer_id);
        connections.sort_by_strategy(self.connection_selection_strategy);
        Ok(connections)
    }

    async fn acquire_connect_lock(
        &self,
        peer_id: PeerId,
    ) -> Result<OwnedMutexGuard<()>, ConnectError> {
        let lock = {
            let mut locks = self.connect_locks.lock();
            locks
                .entry(peer_id)
                .or_insert_with(|| Arc::new(AsyncMutex::new(())))
                .clone()
        };

        tokio::time::timeout(CONNECT_TIMEOUT, lock.lock_owned())
            .await
            .map_err(|_| ConnectError::DialTimeout { peer_id })
    }

    fn spawn_relay_dial_task(
        &self,
        peer_id: PeerId,
        delay: Duration,
        relay_submit_failed: Arc<AtomicBool>,
    ) -> tokio::task::JoinHandle<()> {
        let self_clone = self.clone();
        tokio::spawn(async move {
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }

            if let Err(error) = self_clone.dial_relay_addresses(peer_id).await {
                relay_submit_failed.store(true, Ordering::Relaxed);
                log::debug!(
                    "Relay dial submission for peer {} failed: {}",
                    peer_id,
                    error
                );
            }
        })
    }

    async fn dial_relay_addresses(&self, peer_id: PeerId) -> Result<(), ConnectError> {
        if self.relay_peers.is_empty() {
            log::info!("No relay peers available");
            return Err(ConnectError::NoDialAddresses { peer_id });
        }
        let relay_addresses = self.relay_peers.circuit_addresses_for_target(peer_id);
        self.prepare_for_relay_fallback(peer_id).await;
        self.dial_with_addresses(peer_id, relay_addresses, PeerCondition::Always)
            .await
    }

    async fn dial(&self, opts: DialOpts) -> Result<(), ConnectError> {
        let dial_result = self.invoke_swarm(move |swarm| swarm.dial(opts)).await;
        match dial_result {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(ConnectError::DialFailed(e)),
            Err(e) => Err(ConnectError::SwarmInvocationFailed(e)),
        }
    }

    async fn dial_with_addresses(
        &self,
        peer_id: PeerId,
        addresses: Vec<Multiaddr>,
        condition: PeerCondition,
    ) -> Result<(), ConnectError> {
        if addresses.is_empty() {
            return Err(ConnectError::NoDialAddresses { peer_id });
        }
        let dial_opts = DialOpts::peer_id(peer_id)
            .condition(condition)
            .addresses(addresses)
            .build();
        self.dial(dial_opts).await
    }

    async fn wait_for_state<F>(&self, timeout: Duration, predicate: F) -> bool
    where
        F: Fn(&State) -> bool,
    {
        let deadline = Instant::now() + timeout;
        loop {
            if predicate(&self.state) {
                return true;
            }

            if Instant::now() >= deadline {
                return false;
            }

            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    pub async fn invoke_swarm<F, R: Any + Send>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&mut TSwarm) -> R + Send + Sync + 'static,
    {
        let result = AsyncResult::with(move |completer| {
            self.send(SwarmAsyncCall::new(
                Box::new(|swarm| Box::new(f(swarm))),
                completer,
            ))
            .ok();
        })
        .await
        .map_err(|error| anyhow::anyhow!("Swarm call failed: {:?}", error))?
        .downcast::<R>()
        .map_err(|_| anyhow::anyhow!("Swarm call failed: downcast error"))?;
        Ok(*result)
    }
}
