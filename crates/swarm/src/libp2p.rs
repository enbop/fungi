use crate::{
    ConnectionDirection, ConnectionGovernanceInfo, ConnectionGovernanceState,
    ExternalAddressSource, State, StreamObservationHandle,
    behaviours::{FungiBehaviours, FungiBehavioursEvent},
    peer_handshake::PeerHandshakePayload,
    ping::{PING_PROTOCOL, PingRttEvent, PingState, send_ping_with_timeout},
    state,
};
use anyhow::{Result, bail};
use async_result::{AsyncResult, Completer};
use fungi_util::protocols::{FUNGI_PEER_HANDSHAKE_PROTOCOL, FUNGI_RELAY_HANDSHAKE_PROTOCOL};
use libp2p::{
    Multiaddr, PeerId, Stream, StreamProtocol, Swarm,
    futures::{AsyncReadExt, AsyncWriteExt, StreamExt},
    identify,
    identity::Keypair,
    mdns,
    multiaddr::Protocol,
    noise, relay,
    swarm::{
        ConnectionId, DialError, SwarmEvent,
        dial_opts::{DialOpts, PeerCondition},
    },
    tcp, yamux,
};
use std::{
    any::Any,
    collections::HashMap,
    ops::Deref,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime},
};
use thiserror::Error;
use tokio::{
    sync::mpsc::{self, UnboundedReceiver, UnboundedSender},
    task::JoinHandle,
};

// Relay connection retry constants
const RELAY_RETRY_MAX_ATTEMPTS: u32 = 4;
const RELAY_RETRY_BASE_DELAY_MS: u64 = 500;
const RELAY_HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(600);
const CONNECTION_GOVERNANCE_INTERVAL: Duration = Duration::from_secs(60);
const CONNECTION_GOVERNANCE_GRACE_PERIOD: Duration = Duration::from_secs(120);
/// Simple RAII guard to ensure atomic bool is reset when task completes
struct TaskGuard {
    flag: Arc<AtomicBool>,
}

impl TaskGuard {
    /// Try to acquire the task lock atomically (set to true)
    fn try_acquire(flag: Arc<AtomicBool>) -> Option<Self> {
        if flag
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            Some(Self { flag })
        } else {
            None
        }
    }
}

impl Drop for TaskGuard {
    fn drop(&mut self) {
        self.flag.store(false, Ordering::SeqCst);
    }
}

#[derive(Error, Debug)]
pub enum ConnectError {
    #[error("Dial failed: {0}")]
    DialFailed(#[from] DialError),
    #[error("Already dialing peer {peer_id}")]
    AlreadyDialing { peer_id: PeerId },
    #[error("Swarm invocation failed: {0}")]
    SwarmInvocationFailed(anyhow::Error),
    #[error("Handshake failed: {0}")]
    HandshakeFailed(anyhow::Error),
    #[error("Connection cancelled")]
    Cancelled,
}

pub type TSwarm = Swarm<FungiBehaviours>;
type SwarmResponse = Box<dyn Any + Send>;
type SwarmRequest = Box<dyn FnOnce(&mut TSwarm) -> SwarmResponse + Send + Sync>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RelayTransportKind {
    Tcp,
    Udp,
}

#[derive(Clone)]
struct RelayEndpoint {
    addr: Multiaddr,
    task_flag: Arc<AtomicBool>,
}

impl RelayEndpoint {
    fn new(addr: Multiaddr) -> Self {
        Self {
            addr,
            task_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    fn addr(&self) -> &Multiaddr {
        &self.addr
    }

    fn task_flag(&self) -> &Arc<AtomicBool> {
        &self.task_flag
    }

    fn peer_id(&self) -> Option<PeerId> {
        self.addr.iter().find_map(|protocol| match protocol {
            Protocol::P2p(peer_id) => Some(peer_id),
            _ => None,
        })
    }

    fn listener_prefix(&self) -> Multiaddr {
        self.addr.clone().with(Protocol::P2pCircuit)
    }

    fn matches_listener(&self, listener_addr: &Multiaddr) -> bool {
        multiaddr_starts_with(listener_addr, &self.listener_prefix())
    }

    fn transport_kind(&self) -> Option<RelayTransportKind> {
        relay_transport_kind(&self.addr)
    }

    fn matches_transport(&self, remote_addr: &Multiaddr) -> bool {
        self.transport_kind()
            .zip(relay_transport_kind(remote_addr))
            .is_some_and(|(left, right)| left == right)
    }
}

fn relay_transport_kind(addr: &Multiaddr) -> Option<RelayTransportKind> {
    for protocol in addr.iter() {
        match protocol {
            Protocol::Tcp(_) => return Some(RelayTransportKind::Tcp),
            Protocol::Udp(_) => return Some(RelayTransportKind::Udp),
            _ => {}
        }
    }

    None
}

fn multiaddr_starts_with(addr: &Multiaddr, prefix: &Multiaddr) -> bool {
    let mut addr_iter = addr.iter();
    for prefix_protocol in prefix.iter() {
        let Some(addr_protocol) = addr_iter.next() else {
            return false;
        };

        if addr_protocol != prefix_protocol {
            return false;
        }
    }

    true
}

pub struct SwarmAsyncCall {
    request: SwarmRequest,
    response: Completer<SwarmResponse>,
}

impl SwarmAsyncCall {
    pub fn new(request: SwarmRequest, response: Completer<SwarmResponse>) -> Self {
        Self { request, response }
    }
}

impl Deref for SwarmControl {
    type Target = UnboundedSender<SwarmAsyncCall>;

    fn deref(&self) -> &Self::Target {
        &self.swarm_caller_tx
    }
}

#[derive(Clone)]
pub struct SwarmControl {
    local_peer_id: Arc<PeerId>,
    swarm_caller_tx: UnboundedSender<SwarmAsyncCall>,
    stream_control: fungi_stream::Control,

    /// Relay server addresses with connection state tracking
    ///
    /// Each entry contains:
    /// - Multiaddr: relay server network address
    /// - Arc<AtomicBool>: task running state flag
    ///   - true: connection task is currently running
    ///   - false: idle, ready for new connection attempts
    ///
    /// Prevents duplicate connection attempts to the same relay server
    relay_endpoints: Arc<Vec<RelayEndpoint>>,

    pub(crate) ping_state: Arc<PingState>,

    state: State,
}

#[derive(Debug, Clone, Copy)]
pub enum ConnectionSelectionStrategy {
    PreferDirect,
    PreferRelay,
    PreferLowLatency,
}

#[derive(Debug, Clone)]
pub struct SelectedConnection {
    pub connection_id: ConnectionId,
    pub direction: ConnectionDirection,
    pub remote_addr: Multiaddr,
    pub is_relay: bool,
    pub last_rtt: Option<Duration>,
    pub active_stream_count: usize,
    pub established_at: Option<SystemTime>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
struct ConnectionClosurePlan {
    connection_id: ConnectionId,
    recommended_connection_id: ConnectionId,
    action: GovernanceAction,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum GovernanceAction {
    MarkDeprecated,
    MarkClosing,
    CloseNow,
}

impl SwarmControl {
    /// Create a new swarm control handle.
    pub fn new(
        local_peer_id: Arc<PeerId>,
        swarm_caller_tx: UnboundedSender<SwarmAsyncCall>,
        stream_control: fungi_stream::Control,
        relay_addresses: Vec<Multiaddr>,
        ping_state: Arc<PingState>,
        state: State,
    ) -> Self {
        let relay_endpoints: Arc<Vec<RelayEndpoint>> = Arc::new(
            relay_addresses
                .into_iter()
                .map(RelayEndpoint::new)
                .collect(),
        );
        for relay_endpoint in relay_endpoints.iter() {
            state.register_relay_endpoint(relay_endpoint.addr().clone());
        }
        Self {
            local_peer_id,
            swarm_caller_tx,
            stream_control,
            relay_endpoints,
            ping_state,
            state,
        }
    }

    pub fn local_peer_id(&self) -> PeerId {
        *self.local_peer_id
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    /// Register acceptance of inbound streams for a protocol.
    ///
    /// This is the unified external entrypoint for inbound stream handling,
    /// so daemon-side controls do not need direct access to `stream_control`.
    pub fn accept_incoming_streams(
        &self,
        protocol: StreamProtocol,
    ) -> std::result::Result<fungi_stream::IncomingStreams, fungi_stream::AlreadyRegistered> {
        let mut stream_control = self.stream_control.clone();
        stream_control.listen(protocol)
    }

    /// Ensure peer is connected and collect currently active connections,
    /// then sort them using the requested strategy.
    async fn connect_with_strategy(
        &self,
        peer_id: PeerId,
        strategy: ConnectionSelectionStrategy,
        sniff_wait: Duration,
    ) -> Result<Vec<SelectedConnection>> {
        self.connect(peer_id)
            .await
            .map_err(|e| anyhow::anyhow!("Connect failed: {e}"))?;

        if matches!(
            strategy,
            ConnectionSelectionStrategy::PreferDirect
                | ConnectionSelectionStrategy::PreferLowLatency
        ) && !sniff_wait.is_zero()
        {
            let deadline = tokio::time::Instant::now() + sniff_wait;
            loop {
                let current = self.collect_selected_connections(peer_id);
                if current.iter().any(|c| !c.is_relay) || tokio::time::Instant::now() >= deadline {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }

        let mut selected = self.collect_selected_connections(peer_id);
        if selected.is_empty() {
            bail!("No active connections for peer {peer_id}");
        }

        Self::sort_selected_connections(strategy, &mut selected);
        Ok(selected)
    }

    /// Ping a specific connection and update cached RTT.
    ///
    /// If direct ping on the given connection fails, this falls back to the
    /// unified stream-open entrypoint and retries ping on a recovered stream.
    pub async fn ping_connection(
        &self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        timeout: Duration,
    ) -> Result<Duration> {
        let mapped_peer_id = self
            .state
            .peer_id_by_connection_id(&connection_id)
            .ok_or_else(|| anyhow::anyhow!("Connection {connection_id:?} not found"))?;

        if mapped_peer_id != peer_id {
            bail!("Connection {connection_id:?} belongs to {mapped_peer_id}, not {peer_id}");
        }

        let rtt = self
            .ping_state
            .ping_now(peer_id, connection_id, timeout)
            .await;

        let rtt = match rtt {
            Ok(rtt) => rtt,
            Err(first_err) => {
                log::warn!(
                    "Ping on connection {:?} to {} failed: {}. Retrying via unified stream open path.",
                    connection_id,
                    peer_id,
                    first_err
                );

                let (mut stream, _stream_observation_handle, recovered_connection_id) = self
                    .open_stream_with_strategy(
                        peer_id,
                        PING_PROTOCOL,
                        ConnectionSelectionStrategy::PreferLowLatency,
                        Duration::from_millis(300),
                    )
                    .await
                    .map_err(|e| {
                        anyhow::anyhow!(
                            "Primary ping failed: {first_err}; recovery stream open failed: {e}"
                        )
                    })?;
                stream.ignore_for_keep_alive();
                let recovered_rtt = send_ping_with_timeout(&mut stream, peer_id, timeout)
                    .await
                    .map_err(|e| {
                        anyhow::anyhow!(
                            "Primary ping failed: {first_err}; recovery ping on connection {:?} failed: {}",
                            recovered_connection_id,
                            e
                        )
                    })?;
                self.state
                    .update_connection_ping(&recovered_connection_id, recovered_rtt);
                return Ok(recovered_rtt);
            }
        };
        self.state.update_connection_ping(&connection_id, rtt);
        Ok(rtt)
    }

    pub async fn close_connection(&self, connection_id: ConnectionId) -> Result<bool> {
        self.invoke_swarm(move |swarm| swarm.close_connection(connection_id))
            .await
    }

    /// Unified outbound stream-open API used by ping, file transfer, tunneling,
    /// and future stream-based features.
    ///
    /// Behavior:
    /// - Select candidate connections according to strategy.
    /// - Try opening stream on each candidate.
    /// - On full failure, force one redial and retry once.
    pub async fn open_stream_with_strategy(
        &self,
        target_peer: PeerId,
        target_protocol: StreamProtocol,
        strategy: ConnectionSelectionStrategy,
        sniff_wait: Duration,
    ) -> Result<(Stream, StreamObservationHandle, ConnectionId)> {
        let mut stream_control = self.stream_control.clone();
        let mut last_error_detail = String::from("no candidate connections returned");

        for attempt in 0..2 {
            if attempt == 1 {
                log::info!(
                    "Retrying stream open to peer {} after forced redial",
                    target_peer
                );
                if let Err(e) = self.connect_force_redial(target_peer).await {
                    log::warn!("Forced redial to peer {} failed: {}", target_peer, e);
                }
                tokio::time::sleep(Duration::from_millis(300)).await;
            }

            let candidates = match self
                .connect_with_strategy(target_peer, strategy, sniff_wait)
                .await
            {
                Ok(candidates) => candidates,
                Err(e) => {
                    last_error_detail = e.to_string();
                    continue;
                }
            };

            for selected in &candidates {
                match stream_control
                    .open_stream(selected.connection_id, target_protocol.clone())
                    .await
                {
                    Ok(stream) => {
                        let stream_observation_handle = self.state.track_outbound_stream_opened(
                            target_peer,
                            selected.connection_id,
                            target_protocol.to_string(),
                        );
                        return Ok((stream, stream_observation_handle, selected.connection_id));
                    }
                    Err(e) => {
                        log::warn!(
                            "Failed to open stream on connection {} to peer {} (relay={}, addr={}): {}",
                            selected.connection_id,
                            target_peer,
                            selected.is_relay,
                            selected.remote_addr,
                            e
                        );
                        last_error_detail = e.to_string();
                    }
                }
            }
        }

        bail!(
            "Failed to open stream to peer {} using selected connections: {}",
            target_peer,
            last_error_detail
        )
    }

    fn collect_selected_connections(&self, peer_id: PeerId) -> Vec<SelectedConnection> {
        let Some(peer_connections) = self.state.get_peer_connections(&peer_id) else {
            return Vec::new();
        };

        let mut selected = Vec::new();
        for conn in peer_connections.outbound() {
            let ping_info = self.state.connection_ping_info(&conn.connection_id());
            let last_rtt = ping_info.and_then(|info| info.last_rtt);
            let active_stream_count = self
                .state
                .active_streams_by_connection(&conn.connection_id())
                .len();
            let established_at = self.state.connection_established_at(&conn.connection_id());
            let remote_addr = conn.multiaddr().clone();
            let is_relay = remote_addr.to_string().contains("/p2p-circuit");
            selected.push(SelectedConnection {
                connection_id: conn.connection_id(),
                direction: ConnectionDirection::Outbound,
                remote_addr,
                is_relay,
                last_rtt,
                active_stream_count,
                established_at,
            });
        }

        for conn in peer_connections.inbound() {
            let ping_info = self.state.connection_ping_info(&conn.connection_id());
            let last_rtt = ping_info.and_then(|info| info.last_rtt);
            let active_stream_count = self
                .state
                .active_streams_by_connection(&conn.connection_id())
                .len();
            let established_at = self.state.connection_established_at(&conn.connection_id());
            let remote_addr = conn.multiaddr().clone();
            let is_relay = remote_addr.to_string().contains("/p2p-circuit");
            selected.push(SelectedConnection {
                connection_id: conn.connection_id(),
                direction: ConnectionDirection::Inbound,
                remote_addr,
                is_relay,
                last_rtt,
                active_stream_count,
                established_at,
            });
        }

        selected
    }

    fn sort_selected_connections(
        strategy: ConnectionSelectionStrategy,
        selected: &mut [SelectedConnection],
    ) {
        selected.sort_by(|a, b| match strategy {
            ConnectionSelectionStrategy::PreferDirect => a
                .is_relay
                .cmp(&b.is_relay)
                .then(b.active_stream_count.cmp(&a.active_stream_count))
                .then(Self::rtt_key(a.last_rtt).cmp(&Self::rtt_key(b.last_rtt)))
                .then(
                    Self::established_at_key(a.established_at)
                        .cmp(&Self::established_at_key(b.established_at)),
                )
                .then(Self::conn_id_key(a.connection_id).cmp(&Self::conn_id_key(b.connection_id))),
            ConnectionSelectionStrategy::PreferRelay => b
                .is_relay
                .cmp(&a.is_relay)
                .then(b.active_stream_count.cmp(&a.active_stream_count))
                .then(Self::rtt_key(a.last_rtt).cmp(&Self::rtt_key(b.last_rtt)))
                .then(
                    Self::established_at_key(a.established_at)
                        .cmp(&Self::established_at_key(b.established_at)),
                )
                .then(Self::conn_id_key(a.connection_id).cmp(&Self::conn_id_key(b.connection_id))),
            ConnectionSelectionStrategy::PreferLowLatency => Self::rtt_key(a.last_rtt)
                .cmp(&Self::rtt_key(b.last_rtt))
                .then(b.active_stream_count.cmp(&a.active_stream_count))
                .then(a.is_relay.cmp(&b.is_relay))
                .then(
                    Self::established_at_key(a.established_at)
                        .cmp(&Self::established_at_key(b.established_at)),
                )
                .then(Self::conn_id_key(a.connection_id).cmp(&Self::conn_id_key(b.connection_id))),
        });
    }

    fn conn_id_key(id: ConnectionId) -> u64 {
        let s = id.to_string();
        if let Ok(v) = s.parse::<u64>() {
            return v;
        }
        let digits: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
        digits.parse::<u64>().unwrap_or(u64::MAX)
    }

    fn rtt_key(rtt: Option<Duration>) -> u128 {
        rtt.map(|v| v.as_millis()).unwrap_or(u128::MAX)
    }

    fn established_at_key(established_at: Option<SystemTime>) -> Duration {
        established_at
            .and_then(|ts| ts.duration_since(SystemTime::UNIX_EPOCH).ok())
            .unwrap_or(Duration::MAX)
    }

    fn recommended_reason(
        strategy: ConnectionSelectionStrategy,
        selected: &[SelectedConnection],
    ) -> String {
        let Some(recommended) = selected.first() else {
            return "selected-by-policy".to_string();
        };
        let alternatives = &selected[1..];

        match strategy {
            ConnectionSelectionStrategy::PreferDirect
                if !recommended.is_relay
                    && alternatives.iter().any(|candidate| candidate.is_relay) =>
            {
                return "selected-by-prefer-direct".to_string();
            }
            ConnectionSelectionStrategy::PreferRelay
                if recommended.is_relay
                    && alternatives.iter().any(|candidate| !candidate.is_relay) =>
            {
                return "selected-by-prefer-relay".to_string();
            }
            ConnectionSelectionStrategy::PreferLowLatency
                if recommended.last_rtt.is_some()
                    && alternatives.iter().any(|candidate| {
                        Self::rtt_key(recommended.last_rtt) < Self::rtt_key(candidate.last_rtt)
                    }) =>
            {
                return "selected-by-low-latency".to_string();
            }
            _ => {}
        }

        if recommended.active_stream_count > 0
            && alternatives
                .iter()
                .any(|candidate| recommended.active_stream_count > candidate.active_stream_count)
        {
            return "selected-by-active-streams".to_string();
        }

        if recommended.last_rtt.is_some()
            && alternatives.iter().any(|candidate| {
                Self::rtt_key(recommended.last_rtt) < Self::rtt_key(candidate.last_rtt)
            })
        {
            return "selected-by-lower-rtt".to_string();
        }

        if recommended.established_at.is_some()
            && alternatives.iter().any(|candidate| {
                Self::established_at_key(recommended.established_at)
                    < Self::established_at_key(candidate.established_at)
            })
        {
            return "selected-by-earlier-established".to_string();
        }

        match strategy {
            ConnectionSelectionStrategy::PreferDirect => "selected-by-prefer-direct-ordering",
            ConnectionSelectionStrategy::PreferRelay => "selected-by-prefer-relay-ordering",
            ConnectionSelectionStrategy::PreferLowLatency => "selected-by-low-latency-ordering",
        }
        .to_string()
    }

    fn build_connection_closure_plan(
        strategy: ConnectionSelectionStrategy,
        selected: &mut [SelectedConnection],
        active_stream_counts: &HashMap<ConnectionId, usize>,
        governance_info: &HashMap<ConnectionId, ConnectionGovernanceInfo>,
        now: std::time::SystemTime,
    ) -> Vec<ConnectionClosurePlan> {
        if selected.len() <= 1 {
            return Vec::new();
        }

        Self::sort_selected_connections(strategy, selected);
        let recommended_connection_id = selected[0].connection_id;

        selected
            .iter()
            .skip(1)
            .filter_map(|connection| {
                let reason = format!(
                    "lower-priority-than-connection-{}",
                    recommended_connection_id
                );
                let active_stream_count = active_stream_counts
                    .get(&connection.connection_id)
                    .copied()
                    .unwrap_or(0);
                let current_governance = governance_info.get(&connection.connection_id);

                let action =
                    next_governance_action(current_governance, &reason, active_stream_count, now)?;

                Some(ConnectionClosurePlan {
                    connection_id: connection.connection_id,
                    recommended_connection_id,
                    action,
                })
            })
            .collect()
    }

    // TODO impl handshake
    async fn _handshake(&self, peer_id: PeerId) -> Result<()> {
        let (mut stream, _handle, _connection_id) = self
            .open_stream_with_strategy(
                peer_id,
                FUNGI_PEER_HANDSHAKE_PROTOCOL,
                ConnectionSelectionStrategy::PreferLowLatency,
                Duration::ZERO,
            )
            .await
            .map_err(|e| ConnectError::HandshakeFailed(anyhow::anyhow!(e)))?;
        stream
            .write_all(&PeerHandshakePayload::new().to_bytes())
            .await?;
        let mut buf = [0; 512];
        let n = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut buf)).await??;
        let handshake_res = PeerHandshakePayload::from_bytes(&buf[..n])?;
        log::info!(
            "Connected to {} - {}",
            handshake_res.host_name().unwrap_or_default(),
            peer_id
        );
        let peer_connections = self.state.peer_connections();
        let mut peer_connections = peer_connections.lock();
        peer_connections
            .entry(peer_id)
            .or_default()
            .update_handshake(handshake_res);

        Ok(())
    }

    // connect and handshake
    // TODO add a timeout
    /// Best-effort idempotent connect.
    pub async fn connect(&self, peer_id: PeerId) -> Result<(), ConnectError> {
        self.connect_internal(peer_id, false).await
    }

    async fn connect_force_redial(&self, peer_id: PeerId) -> Result<(), ConnectError> {
        self.connect_internal(peer_id, true).await
    }

    /// Internal connect primitive supporting optional forced redial.
    async fn connect_internal(
        &self,
        peer_id: PeerId,
        force_redial_when_connected: bool,
    ) -> Result<(), ConnectError> {
        if self.state.dial_callback().lock().contains_key(&peer_id) {
            log::warn!("Already dialing {peer_id}");
            return Err(ConnectError::AlreadyDialing { peer_id });
        }

        let (completer, res) = AsyncResult::new_split::<std::result::Result<(), DialError>>();

        let relay_endpoints = self.relay_endpoints.clone();
        let dial_result = self
            .invoke_swarm(move |swarm| {
                if swarm.is_connected(&peer_id) && !force_redial_when_connected {
                    log::debug!("Already connected to {peer_id}");
                    completer.complete(Ok(()));
                    return Ok(());
                }

                let direct_dial_result = if force_redial_when_connected {
                    log::info!("Force redialing peer {peer_id}");
                    let dial_opts = DialOpts::peer_id(peer_id)
                        .condition(PeerCondition::Always)
                        .build();
                    swarm.dial(dial_opts)
                } else {
                    log::debug!("Dialing peer {peer_id} directly");
                    swarm.dial(peer_id)
                };

                if let Err(e) = direct_dial_result {
                    match e {
                        DialError::NoAddresses => {
                            if relay_endpoints.is_empty() {
                                log::warn!("No addresses to dial {peer_id} and no relay addresses available");
                                return Err(DialError::NoAddresses);
                            }
                            // TODO: add a rendezvous server
                            // Fall back to relay addresses when no direct addresses are known.
                            log::info!(
                                "No direct addresses for {peer_id}; dialing with relay addresses {:?}",
                                relay_endpoints
                                    .iter()
                                    .map(|endpoint| endpoint.addr())
                                    .collect::<Vec<_>>()
                            );
                            let mut full_addrs = Vec::new();
                            for endpoint in relay_endpoints.iter() {
                                full_addrs.push(peer_addr_with_relay(peer_id, endpoint.addr().clone()));
                            }
                            let mut dial_opts = DialOpts::peer_id(peer_id).addresses(full_addrs);
                            if force_redial_when_connected {
                                dial_opts = dial_opts.condition(PeerCondition::Always);
                            }
                            let dial_opts = dial_opts.build();
                            swarm.dial(dial_opts)?;
                        }
                        _ => return Err(e),
                    }
                };
                swarm
                    .behaviour()
                    .dial_callback()
                    .lock()
                    .insert(peer_id, completer);
                Ok(())
            })
            .await;

        match dial_result {
            Ok(dial_res) => dial_res?,
            Err(e) => {
                log::warn!("Failed to invoke swarm for dial: {e:?}");
                return Err(ConnectError::SwarmInvocationFailed(e));
            }
        }

        // Wait for dial result
        res.await.map_err(|_| ConnectError::Cancelled)??;

        // TODO impl handshake
        // self.handshake(peer_id)
        //     .await
        //     .map_err(ConnectError::HandshakeFailed)?;

        Ok(())
    }

    pub async fn invoke_swarm<F, R: Any + Send>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&mut TSwarm) -> R + Send + Sync + 'static,
    {
        let res = AsyncResult::with(move |completer| {
            self.send(SwarmAsyncCall::new(
                Box::new(|swarm| Box::new(f(swarm))),
                completer,
            ))
            .ok(); // should be ok cause the completer will be dropped if the channel is closed
        })
        .await
        .map_err(|e| anyhow::anyhow!("Swarm call failed: {:?}", e))?
        .downcast::<R>()
        .map_err(|_| anyhow::anyhow!("Swarm call failed: downcast error"))?;
        Ok(*res)
    }
}

fn next_governance_action(
    current_governance: Option<&ConnectionGovernanceInfo>,
    reason: &str,
    active_stream_count: usize,
    now: std::time::SystemTime,
) -> Option<GovernanceAction> {
    if active_stream_count > 0 {
        return Some(GovernanceAction::MarkDeprecated);
    }

    let Some(current_governance) = current_governance else {
        return Some(GovernanceAction::MarkDeprecated);
    };

    if current_governance.reason.as_deref() != Some(reason) {
        return Some(GovernanceAction::MarkDeprecated);
    }

    match current_governance.state {
        ConnectionGovernanceState::Unknown | ConnectionGovernanceState::Recommended => {
            Some(GovernanceAction::MarkDeprecated)
        }
        ConnectionGovernanceState::Deprecated => {
            let Some(changed_at) = current_governance.changed_at else {
                return Some(GovernanceAction::MarkDeprecated);
            };

            if now.duration_since(changed_at).unwrap_or(Duration::ZERO)
                >= CONNECTION_GOVERNANCE_GRACE_PERIOD
            {
                Some(GovernanceAction::MarkClosing)
            } else {
                None
            }
        }
        ConnectionGovernanceState::Closing => Some(GovernanceAction::CloseNow),
    }
}

pub struct FungiSwarm;

impl FungiSwarm {
    pub async fn start_swarm(
        keypair: Keypair,
        state: State,
        relay_addresses: Vec<Multiaddr>,
        idle_connection_timeout: Duration,
        apply: impl FnOnce(&mut TSwarm),
    ) -> Result<(SwarmControl, JoinHandle<()>)> {
        let mdns =
            mdns::tokio::Behaviour::new(mdns::Config::default(), keypair.public().to_peer_id())?;

        let mut swarm = libp2p::SwarmBuilder::with_existing_identity(keypair)
            .with_tokio()
            .with_tcp(
                tcp::Config::default(),
                noise::Config::new,
                yamux::Config::default,
            )?
            .with_quic()
            .with_relay_client(noise::Config::new, yamux::Config::default)?
            .with_behaviour(|keypair, relay| {
                FungiBehaviours::new(keypair, relay, mdns, state.clone())
            })?
            .with_swarm_config(|c| c.with_idle_connection_timeout(idle_connection_timeout))
            .build();

        let local_peer_id = *swarm.local_peer_id();
        let stream_control = swarm.behaviour().stream.new_control();

        let (ping_event_tx, ping_event_rx) = mpsc::unbounded_channel::<PingRttEvent>();
        let mut ping_state = PingState::new(Duration::from_secs(15), ping_event_tx);
        ping_state.init(stream_control.clone());
        let ping_state = Arc::new(ping_state);

        apply(&mut swarm);

        let (swarm_caller_tx, swarm_caller_rx) = mpsc::unbounded_channel::<SwarmAsyncCall>();
        let (swarm_event_tx, swarm_event_rx) =
            mpsc::unbounded_channel::<SwarmEvent<FungiBehavioursEvent>>();

        let swarm_fut = swarm_loop(swarm, swarm_caller_rx, swarm_event_tx);
        let swarm_control = SwarmControl::new(
            Arc::new(local_peer_id),
            swarm_caller_tx,
            stream_control,
            relay_addresses,
            ping_state,
            state,
        );
        let event_handle_fut = handle_swarm_event(swarm_control.clone(), swarm_event_rx);
        let ping_handle_fut = handle_ping_event(swarm_control.clone(), ping_event_rx);
        let relay_health_fut = relay_management_loop(swarm_control.clone());
        let connection_governance_fut = connection_governance_loop(swarm_control.clone());

        let join_handle = tokio::spawn(async move {
            tokio::select! {
                _ = swarm_fut => {},
                _ = event_handle_fut => {},
                _ = ping_handle_fut => {},
                _ = relay_health_fut => {},
                _ = connection_governance_fut => {},
            }
        });

        Ok((swarm_control, join_handle))
    }
}

async fn swarm_loop(
    mut swarm: TSwarm,
    mut swarm_caller_rx: UnboundedReceiver<SwarmAsyncCall>,
    event_tx: mpsc::UnboundedSender<SwarmEvent<FungiBehavioursEvent>>,
) {
    loop {
        tokio::select! {
            // We use a separate task to handle swarm events, make sure to not block the swarm loop
            swarm_events = swarm.select_next_some() => {
                if let Err(e) = event_tx.send(swarm_events) {
                    log::error!("Failed to send swarm event: {e:?}");
                    break;
                }
            },
            invoke = swarm_caller_rx.recv() => {
                let Some(SwarmAsyncCall{ request, response }) = invoke else {
                    log::debug!("Swarm caller channel closed");
                    break;
                };
                let res = request(&mut swarm);
                response.complete(res);
            }
        }
    }
    log::info!("Swarm loop exited");
}

async fn handle_swarm_event(
    swarm_control: SwarmControl,
    mut event_rx: UnboundedReceiver<SwarmEvent<FungiBehavioursEvent>>,
) {
    loop {
        let Some(event) = event_rx.recv().await else {
            log::debug!("Swarm event channel closed");
            break;
        };
        match event {
            SwarmEvent::NewListenAddr { address, .. } => {
                println!("[Swarm event] NewListenAddr {address:?}");
                handle_new_listen_addr(&swarm_control, address);
            }
            SwarmEvent::ExpiredListenAddr { address, .. } => {
                handle_expired_listen_addr(&swarm_control, address);
            }
            SwarmEvent::ListenerClosed {
                listener_id: _,
                addresses,
                reason,
            } => {
                handle_listener_closed(&swarm_control, addresses, reason);
            }
            SwarmEvent::Behaviour(FungiBehavioursEvent::Mdns(event)) => {
                handle_mdns_behaviour_event(&swarm_control, event);
            }
            SwarmEvent::Behaviour(FungiBehavioursEvent::Identify(event)) => {
                handle_identify_behaviour_event(&swarm_control, event);
            }
            SwarmEvent::Behaviour(FungiBehavioursEvent::Relay(event)) => {
                handle_relay_behaviour_event(&swarm_control, event);
            }
            SwarmEvent::Behaviour(FungiBehavioursEvent::Dcutr(event)) => {
                handle_dcutr_behaviour_event(event);
            }
            SwarmEvent::NewExternalAddrCandidate { address, .. } => {
                swarm_control.state().record_external_address_candidate(
                    address.clone(),
                    ExternalAddressSource::SwarmCandidate,
                );
                log::info!("[Swarm event] NewExternalAddrCandidate {address:?}");
            }
            SwarmEvent::ExternalAddrConfirmed { address } => {
                swarm_control.state().record_external_address_confirmed(
                    address.clone(),
                    ExternalAddressSource::SwarmConfirmed,
                );
                log::info!("[Swarm event] ExternalAddrConfirmed {address:?}");
            }
            SwarmEvent::ExternalAddrExpired { address } => {
                swarm_control.state().expire_external_address(&address);
                log::info!("[Swarm event] ExternalAddrExpired {address:?}");
            }
            SwarmEvent::ConnectionEstablished {
                peer_id,
                connection_id,
                endpoint,
                ..
            } => {
                log::debug!(
                    "Established connection {:?} - peer_id {:?} - multiaddr {:?} - is_dialer {:?}",
                    connection_id,
                    peer_id,
                    endpoint.get_remote_address(),
                    endpoint.is_dialer()
                );

                state::handle_connection_established(
                    &swarm_control,
                    peer_id,
                    connection_id,
                    &endpoint,
                );
                record_relay_connection_established(
                    &swarm_control,
                    peer_id,
                    connection_id,
                    endpoint.get_remote_address(),
                );
            }
            SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                log::info!("[Swarm event] OutgoingConnectionError {peer_id:?}: {error:?}");
                // check dial callback
                let Some(peer_id) = peer_id else {
                    continue;
                };
                handle_outgoing_connection_error(&swarm_control, peer_id, error);
            }
            SwarmEvent::ConnectionClosed {
                peer_id,
                connection_id,
                endpoint,
                cause,
                ..
            } => {
                log::debug!(
                    "Closed connection {} - peer_id {} - multiaddr {:?} - is_dialer {:?} - cause {:?}",
                    connection_id,
                    peer_id,
                    endpoint.get_remote_address(),
                    endpoint.is_dialer(),
                    cause
                );

                record_relay_connection_closed(
                    &swarm_control,
                    peer_id,
                    connection_id,
                    endpoint.get_remote_address(),
                    cause.as_ref().map(|cause| format!("{cause:?}")),
                );
                state::handle_connection_closed(&swarm_control, peer_id, connection_id);
            }
            _ => {}
        }
    }
}

fn handle_mdns_behaviour_event(swarm_control: &SwarmControl, event: mdns::Event) {
    match event {
        mdns::Event::Discovered(entries) => {
            let mut new_count = 0usize;
            let mut refreshed_count = 0usize;
            let mut ignored_count = 0usize;

            for (peer_id, addr) in entries {
                match swarm_control.state().record_peer_address(
                    peer_id,
                    addr,
                    crate::PeerAddressSource::Mdns,
                ) {
                    crate::PeerAddressObservation::New => new_count += 1,
                    crate::PeerAddressObservation::Refreshed => refreshed_count += 1,
                    crate::PeerAddressObservation::Ignored => ignored_count += 1,
                }
            }

            if new_count > 0 || refreshed_count > 0 || ignored_count > 0 {
                log::debug!(
                    "mDNS discovery updated peer address state: new={} refreshed={} ignored={}",
                    new_count,
                    refreshed_count,
                    ignored_count
                );
            }
        }
        mdns::Event::Expired(entries) => {
            let mut expired_count = 0usize;
            let mut missing_count = 0usize;

            for (peer_id, addr) in entries {
                if swarm_control.state().expire_peer_address(peer_id, addr) {
                    expired_count += 1;
                } else {
                    missing_count += 1;
                }
            }

            if expired_count > 0 || missing_count > 0 {
                log::debug!(
                    "mDNS expiry updated peer address state: expired={} missing={}",
                    expired_count,
                    missing_count
                );
            }
        }
    }
}

fn handle_identify_behaviour_event(swarm_control: &SwarmControl, event: identify::Event) {
    match event {
        identify::Event::Received { peer_id, info, .. } => {
            let mut new_addresses = Vec::new();
            let mut refreshed_count = 0usize;
            let mut ignored_count = 0usize;

            for address in info.listen_addrs {
                match swarm_control.state().record_peer_address(
                    peer_id,
                    address.clone(),
                    crate::PeerAddressSource::Identify,
                ) {
                    crate::PeerAddressObservation::New => new_addresses.push(address),
                    crate::PeerAddressObservation::Refreshed => refreshed_count += 1,
                    crate::PeerAddressObservation::Ignored => ignored_count += 1,
                }
            }

            if !new_addresses.is_empty() {
                log::info!(
                    "Identify learned {} new address(es) for peer {}: {}",
                    new_addresses.len(),
                    peer_id,
                    summarize_multiaddrs(&new_addresses)
                );
            }

            if refreshed_count > 0 {
                log::debug!(
                    "Identify refreshed {} existing address(es) for peer {}",
                    refreshed_count,
                    peer_id
                );
            }

            if ignored_count > 0 {
                log::debug!(
                    "Identify ignored {} unusable address(es) for peer {}",
                    ignored_count,
                    peer_id
                );
            }
        }
        identify::Event::Sent { peer_id, .. } => {
            log::debug!("Identify sent to peer {}", peer_id);
        }
        identify::Event::Pushed { peer_id, .. } => {
            log::debug!("Identify pushed update to peer {}", peer_id);
        }
        identify::Event::Error { peer_id, error, .. } => {
            log::debug!("Identify error for peer {}: {}", peer_id, error);
        }
    }
}

fn handle_relay_behaviour_event(swarm_control: &SwarmControl, event: relay::client::Event) {
    match event {
        relay::client::Event::ReservationReqAccepted {
            relay_peer_id,
            renewal,
            ..
        } => {
            swarm_control.state().record_relay_reservation_accepted(
                relay_peer_id,
                if renewal {
                    crate::RelayManagementAction::ReservationRenewed
                } else {
                    crate::RelayManagementAction::ReservationEstablished
                },
            );
            if renewal {
                log::info!("Relay reservation renewed on {relay_peer_id}");
            } else {
                log::info!("Relay reservation established on {relay_peer_id}");
            }
        }
        relay::client::Event::OutboundCircuitEstablished { relay_peer_id, .. } => {
            log::debug!("Outbound relay circuit established via {relay_peer_id}");
        }
        relay::client::Event::InboundCircuitEstablished { src_peer_id, .. } => {
            log::debug!("Inbound relay circuit established from {src_peer_id}");
        }
    }
}

fn handle_dcutr_behaviour_event(event: libp2p::dcutr::Event) {
    match event.result {
        Ok(connection_id) => {
            log::info!(
                "Hole punch succeeded for peer {} on connection {:?}",
                event.remote_peer_id,
                connection_id
            );
        }
        Err(error) => {
            log::warn!(
                "Hole punch failed for peer {}: {}",
                event.remote_peer_id,
                error
            );
        }
    }
}

fn summarize_multiaddrs(addrs: &[Multiaddr]) -> String {
    const PREVIEW_LIMIT: usize = 3;

    let preview = addrs
        .iter()
        .take(PREVIEW_LIMIT)
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ");

    if addrs.len() > PREVIEW_LIMIT {
        format!("{preview}, ... (+{} more)", addrs.len() - PREVIEW_LIMIT)
    } else {
        preview
    }
}

async fn relay_management_loop(swarm_control: SwarmControl) {
    // libp2p renews relay reservations while the direct carrier connection is still healthy.
    // It does not recreate the relay listener after the carrier or listener disappears, so this
    // loop remains as a low-frequency safety net while the event-driven handlers below provide
    // the fast recovery path.
    loop {
        tokio::time::sleep(RELAY_HEALTH_CHECK_INTERVAL).await;

        if swarm_control.relay_endpoints.is_empty() {
            continue;
        }

        let mut endpoint_health = Vec::with_capacity(swarm_control.relay_endpoints.len());

        for relay_endpoint in swarm_control.relay_endpoints.iter() {
            let listener_registered =
                match relay_listener_registered(&swarm_control, relay_endpoint).await {
                    Ok(value) => value,
                    Err(error) => {
                        swarm_control.state().record_relay_management_error(
                            relay_endpoint.addr(),
                            error.to_string(),
                        );
                        log::warn!(
                            "Failed to inspect relay listener state for {}: {}",
                            relay_endpoint.addr(),
                            error
                        );
                        false
                    }
                };

            let has_active_direct_connection = swarm_control
                .state()
                .relay_endpoint_has_active_direct_connection(relay_endpoint.addr());

            swarm_control
                .state()
                .record_relay_listener_check(relay_endpoint.addr(), listener_registered);

            endpoint_health.push((
                relay_endpoint.clone(),
                listener_registered,
                has_active_direct_connection,
            ));
        }

        let mut relay_peer_has_healthy_endpoint = HashMap::<PeerId, bool>::new();
        for (relay_endpoint, listener_registered, has_active_direct_connection) in &endpoint_health
        {
            let Some(relay_peer_id) = relay_endpoint.peer_id() else {
                continue;
            };

            let entry = relay_peer_has_healthy_endpoint
                .entry(relay_peer_id)
                .or_insert(false);
            *entry |= *listener_registered && *has_active_direct_connection;
        }

        for (relay_endpoint, listener_registered, has_active_direct_connection) in endpoint_health {
            if listener_registered && has_active_direct_connection {
                continue;
            }

            let sibling_endpoint_healthy = relay_endpoint
                .peer_id()
                .and_then(|relay_peer_id| {
                    relay_peer_has_healthy_endpoint.get(&relay_peer_id).copied()
                })
                .unwrap_or(false);

            if sibling_endpoint_healthy {
                log::debug!(
                    "Skipping relay reconcile for {} because another endpoint for the same relay peer is already healthy",
                    relay_endpoint.addr()
                );
                continue;
            }

            let action = if !listener_registered {
                crate::RelayManagementAction::ListenerMissingReconcile
            } else {
                crate::RelayManagementAction::DirectConnectionMissingReconcile
            };
            swarm_control
                .state()
                .record_relay_management_action(relay_endpoint.addr(), action);
            log::info!(
                "Relay audit re-establishing reservation for {} because listener_registered={} active_direct_connection={}",
                relay_endpoint.addr(),
                listener_registered,
                has_active_direct_connection
            );
            spawn_relay_listen_task(&swarm_control, &relay_endpoint);
        }
    }
}

async fn connection_governance_loop(swarm_control: SwarmControl) {
    let mut interval = tokio::time::interval(CONNECTION_GOVERNANCE_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        interval.tick().await;
        let now = std::time::SystemTime::now();

        let peer_ids = {
            let peer_connections = swarm_control.state().peer_connections();
            peer_connections.lock().keys().copied().collect::<Vec<_>>()
        };

        for peer_id in peer_ids {
            let mut selected = swarm_control.collect_selected_connections(peer_id);
            if selected.is_empty() {
                continue;
            }

            SwarmControl::sort_selected_connections(
                ConnectionSelectionStrategy::PreferDirect,
                &mut selected,
            );

            let recommended_connection_id = selected[0].connection_id;
            let recommended_reason = SwarmControl::recommended_reason(
                ConnectionSelectionStrategy::PreferDirect,
                &selected,
            );
            swarm_control.state().update_connection_governance(
                &recommended_connection_id,
                ConnectionGovernanceState::Recommended,
                Some(recommended_reason),
            );

            if selected.len() <= 1 {
                continue;
            }

            let active_stream_counts = selected
                .iter()
                .map(|connection| {
                    (
                        connection.connection_id,
                        swarm_control
                            .state()
                            .active_streams_by_connection(&connection.connection_id)
                            .len(),
                    )
                })
                .collect::<HashMap<_, _>>();

            let governance_info = selected
                .iter()
                .map(|connection| {
                    (
                        connection.connection_id,
                        swarm_control
                            .state()
                            .connection_governance_info(&connection.connection_id)
                            .unwrap_or_default(),
                    )
                })
                .collect::<HashMap<_, _>>();

            let closure_plan = SwarmControl::build_connection_closure_plan(
                ConnectionSelectionStrategy::PreferDirect,
                &mut selected,
                &active_stream_counts,
                &governance_info,
                now,
            );

            for plan in closure_plan {
                let active_stream_count = active_stream_counts
                    .get(&plan.connection_id)
                    .copied()
                    .unwrap_or(0);
                let reason = format!(
                    "lower-priority-than-connection-{}",
                    plan.recommended_connection_id
                );

                match plan.action {
                    GovernanceAction::MarkDeprecated => {
                        swarm_control.state().update_connection_governance(
                            &plan.connection_id,
                            ConnectionGovernanceState::Deprecated,
                            Some(reason),
                        );
                    }
                    GovernanceAction::MarkClosing => {
                        log::info!(
                            "Marking deprecated idle connection {} for peer {} as closing in favor of recommended connection {} (active_streams={})",
                            plan.connection_id,
                            peer_id,
                            plan.recommended_connection_id,
                            active_stream_count
                        );
                        swarm_control.state().update_connection_governance(
                            &plan.connection_id,
                            ConnectionGovernanceState::Closing,
                            Some(reason),
                        );
                    }
                    GovernanceAction::CloseNow => {
                        log::warn!(
                            "Closing deprecated idle connection {} for peer {} in favor of recommended connection {} (active_streams={})",
                            plan.connection_id,
                            peer_id,
                            plan.recommended_connection_id,
                            active_stream_count
                        );

                        match swarm_control.close_connection(plan.connection_id).await {
                            Ok(true) => {
                                log::info!(
                                    "Closed deprecated idle connection {} for peer {}",
                                    plan.connection_id,
                                    peer_id
                                );
                            }
                            Ok(false) => {
                                log::debug!(
                                    "Connection {} for peer {} was already gone before governance close",
                                    plan.connection_id,
                                    peer_id
                                );
                            }
                            Err(error) => {
                                log::warn!(
                                    "Failed to close deprecated idle connection {} for peer {}: {}",
                                    plan.connection_id,
                                    peer_id,
                                    error
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

async fn handle_ping_event(
    swarm_control: SwarmControl,
    mut ping_event_rx: UnboundedReceiver<PingRttEvent>,
) {
    loop {
        let Some(event) = ping_event_rx.recv().await else {
            log::debug!("Ping event channel closed");
            break;
        };
        swarm_control
            .state()
            .update_connection_ping(&event.connection_id, event.rtt);
    }
}

fn handle_outgoing_connection_error(
    swarm_control: &SwarmControl,
    peer_id: PeerId,
    error: DialError,
) {
    if let Some(completer) = swarm_control
        .state()
        .dial_callback()
        .lock()
        .remove(&peer_id)
    {
        completer.complete(Err(error));
    }
}

fn handle_new_listen_addr(swarm_control: &SwarmControl, new_addr: Multiaddr) {
    if new_addr.to_string().contains("p2p-circuit") {
        // A relayed listen address means libp2p successfully (re)established the reservation.
        // Mark the endpoint healthy and let libp2p keep owning steady-state renewal from here.
        record_matching_relay_listener_state(swarm_control, &new_addr, true);
        return;
    }
    let mut new_addr_iter = new_addr.iter();

    let should_listen_relay = match new_addr_iter.next() {
        Some(Protocol::Ip4(addr)) => {
            !addr.is_loopback()
                && !addr.is_broadcast()
                && !addr.is_multicast()
                && !addr.is_unspecified()
        }
        Some(Protocol::Ip6(addr)) => {
            !addr.is_loopback()
                && !addr.is_unicast_link_local()
                && !addr.is_unique_local()
                && !addr.is_multicast()
                && !addr.is_unspecified()
        }
        _ => false,
    };
    if should_listen_relay {
        let relay_endpoints = swarm_control.relay_endpoints.clone();

        match relay_transport_kind(&new_addr) {
            Some(RelayTransportKind::Tcp) => {
                for relay_endpoint in relay_endpoints.iter() {
                    if relay_endpoint.transport_kind() != Some(RelayTransportKind::Tcp) {
                        continue;
                    }
                    spawn_relay_listen_task(swarm_control, relay_endpoint);
                }
            }
            Some(RelayTransportKind::Udp) => {
                for relay_endpoint in relay_endpoints.iter() {
                    if relay_endpoint.transport_kind() != Some(RelayTransportKind::Udp) {
                        continue;
                    }
                    spawn_relay_listen_task(swarm_control, relay_endpoint);
                }
            }
            _ => {}
        }
    }
}

fn handle_expired_listen_addr(swarm_control: &SwarmControl, expired_addr: Multiaddr) {
    // Relay listen-address expiry is an early signal that the reservation is no longer externally
    // usable. Reconcile immediately instead of waiting for the periodic audit loop.
    let matched_endpoints =
        record_matching_relay_listener_state(swarm_control, &expired_addr, false);
    for relay_endpoint in matched_endpoints {
        trigger_relay_reconcile(
            swarm_control,
            &relay_endpoint,
            crate::RelayManagementAction::ListenerMissingReconcile,
            format!("relay listen addr expired: {expired_addr}"),
        );
    }
}

fn handle_listener_closed(
    swarm_control: &SwarmControl,
    addresses: Vec<Multiaddr>,
    reason: Result<(), std::io::Error>,
) {
    // `ListenerClosed` is emitted when the relay transport listener shuts down entirely, for
    // example after a relay restart or failed reservation recreation. Treat it as a direct
    // request to recreate the listener on the configured relay endpoint.
    let reason_text = match reason {
        Ok(()) => "listener closed".to_string(),
        Err(error) => error.to_string(),
    };

    for address in addresses {
        let matched_endpoints =
            record_matching_relay_listener_state(swarm_control, &address, false);
        for relay_endpoint in matched_endpoints {
            swarm_control
                .state()
                .record_relay_management_error(relay_endpoint.addr(), reason_text.clone());
            trigger_relay_reconcile(
                swarm_control,
                &relay_endpoint,
                crate::RelayManagementAction::ListenerMissingReconcile,
                format!("relay listener closed at {address}: {reason_text}"),
            );
        }
    }
}

fn record_matching_relay_listener_state(
    swarm_control: &SwarmControl,
    listener_addr: &Multiaddr,
    listener_registered: bool,
) -> Vec<RelayEndpoint> {
    let matched_endpoints = swarm_control
        .relay_endpoints
        .iter()
        .filter(|relay_endpoint| relay_endpoint.matches_listener(listener_addr))
        .cloned()
        .collect::<Vec<_>>();

    for relay_endpoint in &matched_endpoints {
        swarm_control
            .state()
            .record_relay_listener_check(relay_endpoint.addr(), listener_registered);
    }

    matched_endpoints
}

fn trigger_relay_reconcile(
    swarm_control: &SwarmControl,
    relay_endpoint: &RelayEndpoint,
    action: crate::RelayManagementAction,
    reason: String,
) {
    // Multiple transport addresses for the same relay peer act as fallback carriers. If one is
    // still healthy, keep relying on libp2p renewal there instead of re-establishing another
    // redundant reservation on a sibling endpoint.
    if relay_peer_has_healthy_sibling_endpoint(swarm_control, relay_endpoint) {
        log::debug!(
            "Skipping relay reconcile for {} because another endpoint for the same relay peer is already healthy ({})",
            relay_endpoint.addr(),
            reason
        );
        return;
    }

    swarm_control
        .state()
        .record_relay_management_action(relay_endpoint.addr(), action);
    log::info!(
        "Triggering relay reconcile for {}: {}",
        relay_endpoint.addr(),
        reason
    );
    spawn_relay_listen_task(swarm_control, relay_endpoint);
}

fn relay_peer_has_healthy_sibling_endpoint(
    swarm_control: &SwarmControl,
    relay_endpoint: &RelayEndpoint,
) -> bool {
    let Some(relay_peer_id) = relay_endpoint.peer_id() else {
        return false;
    };

    swarm_control
        .state()
        .list_relay_endpoint_statuses()
        .into_iter()
        .any(|status| {
            status.relay_addr != *relay_endpoint.addr()
                && status.relay_peer_id == Some(relay_peer_id)
                && status.listener_registered
                && status.current_direct_connection_id.is_some()
        })
}

fn spawn_relay_listen_task(swarm_control: &SwarmControl, relay_endpoint: &RelayEndpoint) {
    let Some(_guard) = TaskGuard::try_acquire(relay_endpoint.task_flag().clone()) else {
        return;
    };

    swarm_control
        .state()
        .set_relay_task_running(relay_endpoint.addr(), true);
    swarm_control.state().record_relay_management_action(
        relay_endpoint.addr(),
        crate::RelayManagementAction::ListenTaskStarted,
    );

    let swarm_control_cl = swarm_control.clone();
    let relay_addr_cl = relay_endpoint.addr().clone();

    tokio::spawn(async move {
        let _guard = _guard;

        for attempt in 1..=RELAY_RETRY_MAX_ATTEMPTS {
            match listen_relay_by_addr(swarm_control_cl.clone(), relay_addr_cl.clone()).await {
                Ok(()) => {
                    swarm_control_cl.state().record_relay_management_action(
                        &relay_addr_cl,
                        crate::RelayManagementAction::ListenTaskSucceeded,
                    );
                    swarm_control_cl
                        .state()
                        .set_relay_task_running(&relay_addr_cl, false);
                    log::info!(
                        "Successfully connected to relay {relay_addr_cl:?} on attempt {attempt}"
                    );
                    return;
                }
                Err(e) => {
                    swarm_control_cl
                        .state()
                        .record_relay_management_error(&relay_addr_cl, e.to_string());
                    log::warn!(
                        "Failed to connect to relay {relay_addr_cl:?} on attempt {attempt}: {e}"
                    );
                    if attempt < RELAY_RETRY_MAX_ATTEMPTS {
                        let delay =
                            Duration::from_millis(RELAY_RETRY_BASE_DELAY_MS * (1 << (attempt - 1)));
                        tokio::time::sleep(delay).await;
                    }
                }
            }
        }

        swarm_control_cl.state().record_relay_management_action(
            &relay_addr_cl,
            crate::RelayManagementAction::ListenTaskExhausted,
        );
        swarm_control_cl
            .state()
            .set_relay_task_running(&relay_addr_cl, false);
        log::error!(
            "Failed to connect to relay {relay_addr_cl:?} after {RELAY_RETRY_MAX_ATTEMPTS} attempts"
        );
    });
}

async fn relay_listener_registered(
    swarm_control: &SwarmControl,
    relay_endpoint: &RelayEndpoint,
) -> Result<bool> {
    let relay_endpoint = relay_endpoint.clone();
    swarm_control
        .invoke_swarm(move |swarm| {
            swarm
                .listeners()
                .any(|listener_addr| relay_endpoint.matches_listener(listener_addr))
        })
        .await
}

fn record_relay_connection_established(
    swarm_control: &SwarmControl,
    peer_id: PeerId,
    connection_id: ConnectionId,
    remote_addr: &Multiaddr,
) {
    if remote_addr.to_string().contains("/p2p-circuit") {
        return;
    }

    for relay_endpoint in swarm_control.relay_endpoints.iter() {
        let Some(relay_peer_id) = relay_endpoint.peer_id() else {
            continue;
        };

        if relay_peer_id == peer_id && relay_endpoint.matches_transport(remote_addr) {
            swarm_control.state().record_relay_connection_established(
                peer_id,
                connection_id,
                remote_addr,
            );
            log::info!(
                "Relay carrier connection established peer={} transport={} connection_id={} addr={}",
                peer_id,
                relay_endpoint
                    .transport_kind()
                    .map(|kind| match kind {
                        RelayTransportKind::Tcp => "tcp",
                        RelayTransportKind::Udp => "udp",
                    })
                    .unwrap_or("unknown"),
                connection_id,
                remote_addr
            );
        }
    }
}

fn record_relay_connection_closed(
    swarm_control: &SwarmControl,
    peer_id: PeerId,
    connection_id: ConnectionId,
    remote_addr: &Multiaddr,
    cause: Option<String>,
) {
    if remote_addr.to_string().contains("/p2p-circuit") {
        return;
    }

    for relay_endpoint in swarm_control.relay_endpoints.iter() {
        let Some(relay_peer_id) = relay_endpoint.peer_id() else {
            continue;
        };

        if relay_peer_id == peer_id && relay_endpoint.matches_transport(remote_addr) {
            let closed_active_connection = swarm_control.state().record_relay_connection_closed(
                peer_id,
                connection_id,
                remote_addr,
            );
            if closed_active_connection {
                // Once the direct carrier is gone, libp2p can no longer renew the reservation on
                // this relay endpoint. Kick off immediate recovery instead of waiting for audit.
                log::warn!(
                    "Relay carrier connection closed peer={} transport={} connection_id={} addr={} cause={}",
                    peer_id,
                    relay_endpoint
                        .transport_kind()
                        .map(|kind| match kind {
                            RelayTransportKind::Tcp => "tcp",
                            RelayTransportKind::Udp => "udp",
                        })
                        .unwrap_or("unknown"),
                    connection_id,
                    remote_addr,
                    cause.as_deref().unwrap_or("unknown")
                );
                if let Some(cause) = cause.as_deref() {
                    swarm_control
                        .state()
                        .record_relay_management_error(relay_endpoint.addr(), cause.to_string());
                }
                trigger_relay_reconcile(
                    swarm_control,
                    relay_endpoint,
                    crate::RelayManagementAction::DirectConnectionMissingReconcile,
                    format!(
                        "relay carrier connection closed peer={} connection_id={} cause={}",
                        peer_id,
                        connection_id,
                        cause.as_deref().unwrap_or("unknown")
                    ),
                );
            }
        }
    }
}

async fn listen_relay_by_addr(swarm_control: SwarmControl, relay_addr: Multiaddr) -> Result<()> {
    let relay_peer = relay_addr
        .iter()
        .find_map(|p| {
            if let Protocol::P2p(peer_id) = p {
                Some(peer_id)
            } else {
                None
            }
        })
        .ok_or_else(|| anyhow::anyhow!("Invalid relay address"))?;

    // Dialing the relay restores the direct carrier connection required for relay reservation
    // renewal. Without this carrier, libp2p will expire the relay address but will not recreate
    // the listener by itself.
    dial_relay_by_addr(&swarm_control, relay_peer, relay_addr.clone(), false).await?;

    if let Err(error) = perform_relay_handshake(&swarm_control, relay_peer).await {
        log::warn!(
            "Relay handshake to {} via {} failed: {}. Forcing redial.",
            relay_peer,
            relay_addr,
            error
        );
        dial_relay_by_addr(&swarm_control, relay_peer, relay_addr.clone(), true).await?;
        perform_relay_handshake(&swarm_control, relay_peer).await?;
    }

    if relay_listener_registered(&swarm_control, &RelayEndpoint::new(relay_addr.clone())).await? {
        log::debug!("Relay listener already active for {relay_addr}");
        return Ok(());
    };

    // Re-issuing `listen_on(.../p2p-circuit)` is the application-level resurrection step. After
    // this succeeds, libp2p takes back over for steady-state reservation renewal.
    println!("Listening on relay address: {relay_addr:?}");
    swarm_control
        .invoke_swarm(move |swarm| swarm.listen_on(relay_addr.with(Protocol::P2pCircuit)))
        .await??;

    Ok(())
}

async fn dial_relay_by_addr(
    swarm_control: &SwarmControl,
    relay_peer: PeerId,
    relay_addr: Multiaddr,
    force_redial: bool,
) -> Result<()> {
    swarm_control
        .invoke_swarm(move |swarm| {
            if force_redial {
                let dial_opts = DialOpts::peer_id(relay_peer)
                    .addresses(vec![relay_addr.clone()])
                    .condition(PeerCondition::Always)
                    .build();
                if let Err(error) = swarm.dial(dial_opts) {
                    log::warn!(
                        "Failed to force redial relay {relay_peer} via {relay_addr}: {error}"
                    );
                }
                return;
            }

            if !swarm.is_connected(&relay_peer)
                && let Err(error) = swarm.dial(relay_addr.clone())
            {
                log::error!("Failed to dial relay address {relay_peer} via {relay_addr}: {error}");
            }
        })
        .await?;
    Ok(())
}

async fn perform_relay_handshake(swarm_control: &SwarmControl, relay_peer: PeerId) -> Result<()> {
    let Ok(stream_result) = tokio::time::timeout(
        Duration::from_secs(5),
        swarm_control.open_stream_with_strategy(
            relay_peer,
            FUNGI_RELAY_HANDSHAKE_PROTOCOL,
            ConnectionSelectionStrategy::PreferLowLatency,
            Duration::ZERO,
        ),
    )
    .await
    else {
        bail!("Handshake timeout")
    };

    let (mut stream, _handle, _connection_id) = match stream_result {
        Ok(stream) => stream,
        Err(error) => bail!("Handshake failed: {:?}", error),
    };
    let mut buf = [0u8; 32];
    let Ok(read_result) = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut buf)).await
    else {
        bail!("Handshake read timeout")
    };
    let n = read_result?;
    if n < 1 {
        bail!("Handshake failed: empty response");
    }

    Ok(())
}

pub fn get_default_relay_addrs() -> Vec<Multiaddr> {
    vec![
        "/ip4/160.16.206.21/tcp/30001/p2p/16Uiu2HAmGXFS6aYsKKYRkEDo1tNigZKN8TAYrsfSnEdC5sZLNkiE"
            .parse()
            .unwrap(),
        "/ip4/160.16.206.21/udp/30001/quic-v1/p2p/16Uiu2HAmGXFS6aYsKKYRkEDo1tNigZKN8TAYrsfSnEdC5sZLNkiE"
            .parse()
            .unwrap(),
    ]
}

pub fn peer_addr_with_relay(peer_id: PeerId, relay: Multiaddr) -> Multiaddr {
    relay
        .with(Protocol::P2pCircuit)
        .with(Protocol::P2p(peer_id))
}

#[cfg(test)]
mod tests {
    use super::{
        CONNECTION_GOVERNANCE_GRACE_PERIOD, ConnectionSelectionStrategy, GovernanceAction,
        RelayEndpoint, RelayTransportKind, SelectedConnection, SwarmControl, multiaddr_starts_with,
        next_governance_action, relay_transport_kind,
    };
    use crate::{ConnectionDirection, ConnectionGovernanceInfo, ConnectionGovernanceState};
    use libp2p::{Multiaddr, swarm::ConnectionId};
    use std::{
        collections::HashMap,
        time::{Duration, SystemTime},
    };

    #[test]
    fn relay_listener_match_accepts_confirmed_listener_addr() {
        let relay_endpoint = RelayEndpoint::new(
            "/ip4/160.16.206.21/udp/30001/quic-v1/p2p/16Uiu2HAmGXFS6aYsKKYRkEDo1tNigZKN8TAYrsfSnEdC5sZLNkiE"
                .parse()
                .unwrap(),
        );
        let confirmed_listener = "/ip4/160.16.206.21/udp/30001/quic-v1/p2p/16Uiu2HAmGXFS6aYsKKYRkEDo1tNigZKN8TAYrsfSnEdC5sZLNkiE/p2p-circuit/p2p/16Uiu2HAmGXFS6aYsKKYRkEDo1tNigZKN8TAYrsfSnEdC5sZLNkiE"
            .parse()
            .unwrap();

        assert!(relay_endpoint.matches_listener(&confirmed_listener));
    }

    #[test]
    fn multiaddr_prefix_match_rejects_different_transport() {
        let relay_prefix = "/ip4/160.16.206.21/udp/30001/quic-v1/p2p/16Uiu2HAmGXFS6aYsKKYRkEDo1tNigZKN8TAYrsfSnEdC5sZLNkiE/p2p-circuit"
            .parse()
            .unwrap();
        let tcp_listener = "/ip4/160.16.206.21/tcp/30001/p2p/16Uiu2HAmGXFS6aYsKKYRkEDo1tNigZKN8TAYrsfSnEdC5sZLNkiE/p2p-circuit/p2p/16Uiu2HAmGXFS6aYsKKYRkEDo1tNigZKN8TAYrsfSnEdC5sZLNkiE"
            .parse()
            .unwrap();

        assert!(!multiaddr_starts_with(&tcp_listener, &relay_prefix));
    }

    #[test]
    fn relay_transport_kind_detects_quic_as_udp() {
        let addr = "/ip4/160.16.206.21/udp/30001/quic-v1/p2p/16Uiu2HAmGXFS6aYsKKYRkEDo1tNigZKN8TAYrsfSnEdC5sZLNkiE"
            .parse()
            .unwrap();

        assert_eq!(relay_transport_kind(&addr), Some(RelayTransportKind::Udp));
    }

    #[test]
    fn relay_endpoint_matches_transport_by_protocol_kind() {
        let relay_endpoint = RelayEndpoint::new(
            "/ip4/160.16.206.21/tcp/30001/p2p/16Uiu2HAmGXFS6aYsKKYRkEDo1tNigZKN8TAYrsfSnEdC5sZLNkiE"
                .parse()
                .unwrap(),
        );
        let tcp_remote = "/ip4/160.16.206.21/tcp/30001/p2p/16Uiu2HAmGXFS6aYsKKYRkEDo1tNigZKN8TAYrsfSnEdC5sZLNkiE"
            .parse()
            .unwrap();
        let udp_remote = "/ip4/160.16.206.21/udp/30001/quic-v1/p2p/16Uiu2HAmGXFS6aYsKKYRkEDo1tNigZKN8TAYrsfSnEdC5sZLNkiE"
            .parse()
            .unwrap();

        assert!(relay_endpoint.matches_transport(&tcp_remote));
        assert!(!relay_endpoint.matches_transport(&udp_remote));
    }

    fn selected_connection(
        connection_id: usize,
        remote_addr: &str,
        is_relay: bool,
        last_rtt_ms: Option<u64>,
    ) -> SelectedConnection {
        SelectedConnection {
            connection_id: ConnectionId::new_unchecked(connection_id),
            direction: ConnectionDirection::Outbound,
            remote_addr: remote_addr.parse::<Multiaddr>().unwrap(),
            is_relay,
            last_rtt: last_rtt_ms.map(Duration::from_millis),
            active_stream_count: 0,
            established_at: None,
        }
    }

    #[test]
    fn sort_selected_connections_prefers_active_streams_before_rtt() {
        let mut selected = vec![
            SelectedConnection {
                active_stream_count: 0,
                ..selected_connection(8, "/ip4/1.1.1.1/tcp/4002", false, Some(10))
            },
            SelectedConnection {
                active_stream_count: 2,
                ..selected_connection(6, "/ip4/1.1.1.1/tcp/4001", false, Some(30))
            },
        ];

        SwarmControl::sort_selected_connections(
            ConnectionSelectionStrategy::PreferDirect,
            &mut selected,
        );

        assert_eq!(selected[0].connection_id, ConnectionId::new_unchecked(6));
    }

    #[test]
    fn sort_selected_connections_prefers_earlier_established_when_other_signals_match() {
        let earlier = SystemTime::UNIX_EPOCH + Duration::from_secs(10);
        let later = SystemTime::UNIX_EPOCH + Duration::from_secs(20);
        let mut selected = vec![
            SelectedConnection {
                established_at: Some(later),
                ..selected_connection(8, "/ip4/1.1.1.1/tcp/4002", false, Some(30))
            },
            SelectedConnection {
                established_at: Some(earlier),
                ..selected_connection(6, "/ip4/1.1.1.1/tcp/4001", false, Some(30))
            },
        ];

        SwarmControl::sort_selected_connections(
            ConnectionSelectionStrategy::PreferDirect,
            &mut selected,
        );

        assert_eq!(selected[0].connection_id, ConnectionId::new_unchecked(6));
    }

    #[test]
    fn closure_plan_prefers_direct_and_closes_idle_relay() {
        let mut selected = vec![
            selected_connection(9, "/ip4/1.1.1.1/tcp/4001/p2p-circuit", true, Some(20)),
            selected_connection(4, "/ip4/1.1.1.1/tcp/4001", false, Some(50)),
        ];
        let active_stream_counts = HashMap::from([
            (ConnectionId::new_unchecked(9), 0usize),
            (ConnectionId::new_unchecked(4), 0usize),
        ]);
        let governance_info = HashMap::from([
            (
                ConnectionId::new_unchecked(9),
                ConnectionGovernanceInfo {
                    state: ConnectionGovernanceState::Closing,
                    reason: Some("lower-priority-than-connection-4".to_string()),
                    changed_at: Some(SystemTime::now()),
                },
            ),
            (
                ConnectionId::new_unchecked(4),
                ConnectionGovernanceInfo {
                    state: ConnectionGovernanceState::Recommended,
                    reason: Some("selected-by-prefer-direct-baseline".to_string()),
                    changed_at: Some(SystemTime::now()),
                },
            ),
        ]);

        let plan = SwarmControl::build_connection_closure_plan(
            ConnectionSelectionStrategy::PreferDirect,
            &mut selected,
            &active_stream_counts,
            &governance_info,
            SystemTime::now(),
        );

        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].connection_id, ConnectionId::new_unchecked(9));
        assert_eq!(
            plan[0].recommended_connection_id,
            ConnectionId::new_unchecked(4)
        );
        assert_eq!(plan[0].action, GovernanceAction::CloseNow);
    }

    #[test]
    fn closure_plan_keeps_deprecated_connection_when_stream_is_active() {
        let mut selected = vec![
            selected_connection(8, "/ip4/1.1.1.1/tcp/4002", false, Some(90)),
            selected_connection(6, "/ip4/1.1.1.1/tcp/4001", false, Some(15)),
        ];
        let active_stream_counts = HashMap::from([
            (ConnectionId::new_unchecked(8), 2usize),
            (ConnectionId::new_unchecked(6), 0usize),
        ]);
        let governance_info = HashMap::from([
            (
                ConnectionId::new_unchecked(8),
                ConnectionGovernanceInfo {
                    state: ConnectionGovernanceState::Closing,
                    reason: Some("lower-priority-than-connection-6".to_string()),
                    changed_at: Some(SystemTime::now()),
                },
            ),
            (
                ConnectionId::new_unchecked(6),
                ConnectionGovernanceInfo {
                    state: ConnectionGovernanceState::Recommended,
                    reason: Some("selected-by-prefer-direct-baseline".to_string()),
                    changed_at: Some(SystemTime::now()),
                },
            ),
        ]);

        let plan = SwarmControl::build_connection_closure_plan(
            ConnectionSelectionStrategy::PreferDirect,
            &mut selected,
            &active_stream_counts,
            &governance_info,
            SystemTime::now(),
        );

        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].action, GovernanceAction::MarkDeprecated);
    }

    #[test]
    fn next_governance_action_respects_grace_period_before_closing() {
        let now = SystemTime::now();
        let current = ConnectionGovernanceInfo {
            state: ConnectionGovernanceState::Deprecated,
            reason: Some("lower-priority-than-connection-4".to_string()),
            changed_at: Some(now),
        };

        let action =
            next_governance_action(Some(&current), "lower-priority-than-connection-4", 0, now);

        assert_eq!(action, None);
    }

    #[test]
    fn next_governance_action_transitions_to_closing_and_then_close() {
        let now = SystemTime::now();
        let deprecated = ConnectionGovernanceInfo {
            state: ConnectionGovernanceState::Deprecated,
            reason: Some("lower-priority-than-connection-4".to_string()),
            changed_at: Some(now - CONNECTION_GOVERNANCE_GRACE_PERIOD),
        };
        let closing = ConnectionGovernanceInfo {
            state: ConnectionGovernanceState::Closing,
            reason: Some("lower-priority-than-connection-4".to_string()),
            changed_at: Some(now),
        };

        assert_eq!(
            next_governance_action(
                Some(&deprecated),
                "lower-priority-than-connection-4",
                0,
                now,
            ),
            Some(GovernanceAction::MarkClosing)
        );
        assert_eq!(
            next_governance_action(Some(&closing), "lower-priority-than-connection-4", 0, now,),
            Some(GovernanceAction::CloseNow)
        );
    }
}
