use crate::{
    ConnectionDirection, State, StreamObservationHandle,
    behaviours::{FungiBehaviours, FungiBehavioursEvent},
    connection_state,
    peer_handshake::PeerHandshakePayload,
    ping::{PING_PROTOCOL, PingRttEvent, PingState, send_ping_with_timeout},
};
use anyhow::{Result, bail};
use async_result::{AsyncResult, Completer};
use fungi_util::protocols::{FUNGI_PEER_HANDSHAKE_PROTOCOL, FUNGI_RELAY_HANDSHAKE_PROTOCOL};
use libp2p::{
    Multiaddr, PeerId, Stream, StreamProtocol, Swarm,
    futures::{AsyncReadExt, AsyncWriteExt, StreamExt},
    identity::Keypair,
    mdns,
    multiaddr::Protocol,
    noise,
    swarm::{
        ConnectionId, DialError, SwarmEvent,
        dial_opts::{DialOpts, PeerCondition},
    },
    tcp, yamux,
};
use std::{
    any::Any,
    ops::Deref,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use thiserror::Error;
use tokio::{
    sync::mpsc::{self, UnboundedReceiver, UnboundedSender},
    task::JoinHandle,
};

// Relay connection retry constants
const RELAY_RETRY_MAX_ATTEMPTS: u32 = 4;
const RELAY_RETRY_BASE_DELAY_MS: u64 = 500;
const OUTBOUND_PING_INTERVAL: Duration = Duration::from_secs(15);

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
    stream_control: libp2p_stream::Control,

    /// Relay server addresses with connection state tracking
    ///
    /// Each entry contains:
    /// - Multiaddr: relay server network address
    /// - Arc<AtomicBool>: task running state flag
    ///   - true: connection task is currently running
    ///   - false: idle, ready for new connection attempts
    ///
    /// Prevents duplicate connection attempts to the same relay server
    relay_addresses_state: Arc<Vec<(Multiaddr, Arc<AtomicBool>)>>,

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
}

impl SwarmControl {
    /// Create a new swarm control handle.
    pub fn new(
        local_peer_id: Arc<PeerId>,
        swarm_caller_tx: UnboundedSender<SwarmAsyncCall>,
        stream_control: libp2p_stream::Control,
        relay_addresses: Vec<Multiaddr>,
        ping_state: Arc<PingState>,
        state: State,
    ) -> Self {
        let relay_addresses_state = Arc::new(
            relay_addresses
                .into_iter()
                .map(|addr| (addr, Arc::new(AtomicBool::new(false))))
                .collect(),
        );
        Self {
            local_peer_id,
            swarm_caller_tx,
            stream_control,
            relay_addresses_state,
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
    ) -> std::result::Result<libp2p_stream::IncomingStreams, libp2p_stream::AlreadyRegistered> {
        let mut stream_control = self.stream_control.clone();
        stream_control.accept(protocol)
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
                    .open_stream_on_connection(
                        target_peer,
                        selected.connection_id,
                        target_protocol.clone(),
                    )
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
            let remote_addr = conn.multiaddr().clone();
            let is_relay = remote_addr.to_string().contains("/p2p-circuit");
            selected.push(SelectedConnection {
                connection_id: conn.connection_id(),
                direction: ConnectionDirection::Outbound,
                remote_addr,
                is_relay,
                last_rtt,
            });
        }

        for conn in peer_connections.inbound() {
            let ping_info = self.state.connection_ping_info(&conn.connection_id());
            let last_rtt = ping_info.and_then(|info| info.last_rtt);
            let remote_addr = conn.multiaddr().clone();
            let is_relay = remote_addr.to_string().contains("/p2p-circuit");
            selected.push(SelectedConnection {
                connection_id: conn.connection_id(),
                direction: ConnectionDirection::Inbound,
                remote_addr,
                is_relay,
                last_rtt,
            });
        }

        selected
    }

    fn sort_selected_connections(
        strategy: ConnectionSelectionStrategy,
        selected: &mut [SelectedConnection],
    ) {
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

        selected.sort_by(|a, b| match strategy {
            ConnectionSelectionStrategy::PreferDirect => a
                .is_relay
                .cmp(&b.is_relay)
                .then(rtt_key(a.last_rtt).cmp(&rtt_key(b.last_rtt)))
                .then(conn_id_key(a.connection_id).cmp(&conn_id_key(b.connection_id))),
            ConnectionSelectionStrategy::PreferRelay => b
                .is_relay
                .cmp(&a.is_relay)
                .then(rtt_key(a.last_rtt).cmp(&rtt_key(b.last_rtt)))
                .then(conn_id_key(a.connection_id).cmp(&conn_id_key(b.connection_id))),
            ConnectionSelectionStrategy::PreferLowLatency => rtt_key(a.last_rtt)
                .cmp(&rtt_key(b.last_rtt))
                .then(a.is_relay.cmp(&b.is_relay))
                .then(conn_id_key(a.connection_id).cmp(&conn_id_key(b.connection_id))),
        });
    }

    // TODO impl handshake
    async fn _handshake(&self, peer_id: PeerId) -> Result<()> {
        let mut stream = self
            .stream_control
            .clone()
            .open_stream(peer_id, FUNGI_PEER_HANDSHAKE_PROTOCOL)
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

        let relay_addresses_state = self.relay_addresses_state.clone();
        let dial_result = self
            .invoke_swarm(move |swarm| {
                if swarm.is_connected(&peer_id) && !force_redial_when_connected {
                    log::info!("Already connected to {peer_id}");
                    completer.complete(Ok(()));
                    return Ok(());
                }

                let direct_dial_result = if force_redial_when_connected {
                    let dial_opts = DialOpts::peer_id(peer_id)
                        .condition(PeerCondition::Always)
                        .build();
                    swarm.dial(dial_opts)
                } else {
                    swarm.dial(peer_id)
                };

                if let Err(e) = direct_dial_result {
                    match e {
                        DialError::NoAddresses => {
                            if relay_addresses_state.is_empty() {
                                log::warn!("No addresses to dial {peer_id} and no relay addresses available");
                                return Err(DialError::NoAddresses);
                            }
                            // TODO: add a rendezvous server
                            // dial with relay when no mDNS addresses are available
                            log::info!(
                                "Dialing {peer_id} with relay address {:?}",
                                relay_addresses_state.iter().map(|(addr, _)| addr).collect::<Vec<_>>()
                            );
                            let mut full_addrs = Vec::new();
                            for (relay_addr, _) in relay_addresses_state.iter() {
                                full_addrs.push(peer_addr_with_relay(peer_id, relay_addr.clone()));
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
        let mut ping_state = PingState::new(OUTBOUND_PING_INTERVAL, ping_event_tx);
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

        let join_handle = tokio::spawn(async move {
            tokio::select! {
                _ = swarm_fut => {},
                _ = event_handle_fut => {},
                _ = ping_handle_fut => {},
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
            SwarmEvent::NewExternalAddrCandidate { address, .. } => {
                log::info!("[Swarm event] NewExternalAddrCandidate {address:?}");
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

                connection_state::handle_connection_established(
                    &swarm_control,
                    peer_id,
                    connection_id,
                    &endpoint,
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

                connection_state::handle_connection_closed(&swarm_control, peer_id, connection_id);
            }
            _ => {}
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
        let relay_addresses_state = swarm_control.relay_addresses_state.clone();

        fn spawn_listen_task(
            swarm_control: &SwarmControl,
            relay_addr: &Multiaddr,
            is_running: &Arc<AtomicBool>,
        ) {
            let Some(_guard) = TaskGuard::try_acquire(is_running.clone()) else {
                return;
            };

            let swarm_control_cl = swarm_control.clone();
            let relay_addr_cl = relay_addr.clone();

            tokio::spawn(async move {
                // move _guard to this async task
                // and it will set the flag to false when the task is dropped
                let _guard = _guard;

                for attempt in 1..=RELAY_RETRY_MAX_ATTEMPTS {
                    match listen_relay_by_addr(swarm_control_cl.clone(), relay_addr_cl.clone())
                        .await
                    {
                        Ok(()) => {
                            log::info!(
                                "Successfully connected to relay {relay_addr_cl:?} on attempt {attempt}"
                            );
                            return;
                        }
                        Err(e) => {
                            log::warn!(
                                "Failed to connect to relay {relay_addr_cl:?} on attempt {attempt}: {e}"
                            );
                            if attempt < RELAY_RETRY_MAX_ATTEMPTS {
                                let delay = Duration::from_millis(
                                    RELAY_RETRY_BASE_DELAY_MS * (1 << (attempt - 1)),
                                );
                                tokio::time::sleep(delay).await;
                            }
                        }
                    }
                }

                log::error!(
                    "Failed to connect to relay {relay_addr_cl:?} after {RELAY_RETRY_MAX_ATTEMPTS} attempts"
                );
            });
        }

        match new_addr_iter.next() {
            Some(Protocol::Tcp(_)) => {
                for (relay_addr, is_running) in relay_addresses_state.iter() {
                    if !relay_addr.to_string().contains("/tcp/") {
                        continue;
                    }
                    spawn_listen_task(swarm_control, relay_addr, is_running);
                }
            }
            Some(Protocol::Udp(_)) => {
                for (relay_addr, is_running) in relay_addresses_state.iter() {
                    if !relay_addr.to_string().contains("/udp/") {
                        continue;
                    }
                    spawn_listen_task(swarm_control, relay_addr, is_running);
                }
            }
            _ => {}
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

    // 1. dial to relay server will retrieve and update local public address (sent to SwarmEvent::NewExternalAddrCandidate) automatically
    let relay_addr_cl = relay_addr.clone();
    swarm_control
        .invoke_swarm(move |swarm| {
            if !swarm.is_connected(&relay_peer)
                && let Err(e) = swarm.dial(relay_addr_cl)
            {
                log::error!("Failed to dial relay address {relay_peer}: {e}");
            }
        })
        .await?;

    // 2. We have to establish a connection with the relay server before listen_on P2pCircuit
    let Ok(stream_result) = tokio::time::timeout(
        Duration::from_secs(5),
        swarm_control
            .stream_control
            .clone()
            .open_stream(relay_peer, FUNGI_RELAY_HANDSHAKE_PROTOCOL),
    )
    .await
    else {
        bail!("Handshake timeout")
    };
    let mut stream = match stream_result {
        Ok(stream) => stream,
        Err(e) => bail!("Handshake failed: {:?}", e),
    };
    let mut buf = [0u8; 32];
    // TODO
    // implement a proper handshake protocol, currently just read the response to make sure both sides are reachable
    let n = stream.read(&mut buf).await?;
    if n < 1 {
        bail!("Handshake failed: empty response");
    };

    // 3. listen on relay
    println!("Listening on relay address: {relay_addr:?}");
    swarm_control
        .invoke_swarm(move |swarm| swarm.listen_on(relay_addr.with(Protocol::P2pCircuit)))
        .await??;

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
