use super::{
    ConnectionSelectionStrategy, SelectedConnection, TSwarm,
    dial_plan::DialPlan,
    relay::{RefreshThrottle, RelayPeers, is_circuit_addr},
};
use crate::{
    ConnectionDirection, State, StreamObservationHandle,
    ping::{PingState, send_ping_with_timeout},
};
use anyhow::{Result, bail};
use async_result::{AsyncResult, Completer};
use fungi_util::protocols::FUNGI_PROBE_PROTOCOL;
use libp2p::{
    Multiaddr, PeerId, Stream, StreamProtocol,
    swarm::{
        ConnectionId, DialError,
        dial_opts::{DialOpts, PeerCondition},
    },
};
use std::{any::Any, ops::Deref, sync::Arc, time::Duration};
use thiserror::Error;
use tokio::sync::mpsc::UnboundedSender;

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

const DIRECT_DIAL_FALLBACK_DELAY: Duration = Duration::from_millis(500);
const DIRECT_DIAL_TIMEOUT: Duration = Duration::from_secs(5);
const RELAY_DIAL_TIMEOUT: Duration = Duration::from_secs(8);
const STREAM_OPEN_STATE_RETRY_DELAY: Duration = Duration::from_millis(200);

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
    pub(super) refresh_throttle: RefreshThrottle,
    pub(super) relay_peers: RelayPeers,
    pub(crate) ping_state: Arc<PingState>,
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
        ping_state: Arc<PingState>,
        state: State,
    ) -> Self {
        relay_peers.register_with_state(&state);
        Self {
            local_peer_id,
            swarm_caller_tx,
            stream_control,
            refresh_throttle,
            relay_peers,
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

    pub fn accept_incoming_streams(
        &self,
        protocol: StreamProtocol,
    ) -> std::result::Result<fungi_stream::IncomingStreams, fungi_stream::AlreadyRegistered> {
        let mut stream_control = self.stream_control.clone();
        stream_control.listen(protocol)
    }

    async fn connect_with_strategy(
        &self,
        peer_id: PeerId,
        strategy: ConnectionSelectionStrategy,
        sniff_wait: Duration,
    ) -> Result<Vec<SelectedConnection>> {
        self.connect(peer_id)
            .await
            .map_err(|error| anyhow::anyhow!("Connect failed: {error}"))?;

        if matches!(
            strategy,
            ConnectionSelectionStrategy::PreferDirect
                | ConnectionSelectionStrategy::PreferLowLatency
        ) && !sniff_wait.is_zero()
        {
            let deadline = tokio::time::Instant::now() + sniff_wait;
            loop {
                let current = self.collect_selected_connections(peer_id);
                if current.iter().any(|connection| !connection.is_relay)
                    || tokio::time::Instant::now() >= deadline
                {
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
                        FUNGI_PROBE_PROTOCOL,
                        ConnectionSelectionStrategy::PreferLowLatency,
                        Duration::from_millis(300),
                    )
                    .await
                    .map_err(|error| {
                        anyhow::anyhow!(
                            "Primary ping failed: {first_err}; recovery stream open failed: {error}"
                        )
                    })?;
                stream.ignore_for_keep_alive();
                let recovered_rtt = send_ping_with_timeout(&mut stream, peer_id, timeout)
                    .await
                    .map_err(|error| {
                        anyhow::anyhow!(
                            "Primary ping failed: {first_err}; recovery ping on connection {:?} failed: {}",
                            recovered_connection_id,
                            error
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

    pub async fn probe_peer(
        &self,
        peer_id: PeerId,
        timeout: Duration,
    ) -> Result<(Duration, ConnectionId)> {
        let (mut stream, _stream_observation_handle, connection_id) = self
            .open_stream_with_strategy(
                peer_id,
                FUNGI_PROBE_PROTOCOL,
                ConnectionSelectionStrategy::PreferLowLatency,
                Duration::from_millis(300),
            )
            .await?;
        stream.ignore_for_keep_alive();
        let rtt = send_ping_with_timeout(&mut stream, peer_id, timeout).await?;
        self.state.update_connection_ping(&connection_id, rtt);
        Ok((rtt, connection_id))
    }

    pub async fn close_connection(&self, connection_id: ConnectionId) -> Result<bool> {
        self.invoke_swarm(move |swarm| swarm.close_connection(connection_id))
            .await
    }

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
            if attempt > 0 {
                // Do not force another dial here. Connection establishment is
                // owned by connect_with_explicit_plan; stream opening only
                // re-reads current connection state after giving close/open
                // events a short moment to settle.
                tokio::time::sleep(STREAM_OPEN_STATE_RETRY_DELAY).await;
            }

            let candidates = match self
                .connect_with_strategy(target_peer, strategy, sniff_wait)
                .await
            {
                Ok(candidates) => candidates,
                Err(error) => {
                    last_error_detail = error.to_string();
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
                    Err(error) => {
                        log::warn!(
                            "Failed to open stream on connection {} to peer {} (relay={}, addr={}): {}",
                            selected.connection_id,
                            target_peer,
                            selected.is_relay,
                            selected.remote_addr,
                            error
                        );
                        last_error_detail = error.to_string();

                        if matches!(error, fungi_stream::OpenStreamError::UnsupportedProtocol(_)) {
                            bail!(
                                "Failed to open stream to peer {} using selected connections: {}",
                                target_peer,
                                last_error_detail
                            );
                        }
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

    pub async fn connect(&self, peer_id: PeerId) -> Result<(), ConnectError> {
        if self.relay_peers.is_relay_peer(peer_id) {
            return self.connect_configured_relay_peer(peer_id).await;
        }

        match self
            .invoke_swarm(move |swarm| swarm.is_connected(&peer_id))
            .await
        {
            Ok(true) => {
                log::debug!("Already connected to {peer_id}");
                return Ok(());
            }
            Ok(false) => {}
            Err(error) => {
                log::warn!("Failed to inspect connection state for {peer_id}: {error:?}");
                return Err(ConnectError::SwarmInvocationFailed(error));
            }
        }

        self.connect_with_explicit_plan(peer_id).await
    }

    pub(super) async fn connect_configured_relay_peer(
        &self,
        peer_id: PeerId,
    ) -> Result<(), ConnectError> {
        if self.relay_tcp_active(peer_id) {
            log::debug!("Relay peer {peer_id} already has an active TCP carrier");
            return Ok(());
        }

        let Some(relay_addresses) = self.relay_peers.addresses_for_peer(peer_id) else {
            return Err(ConnectError::NoDialAddresses { peer_id });
        };

        log::debug!(
            "Dialing configured relay peer {} through TCP carrier address(es): {}",
            peer_id,
            relay_addresses
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        );

        match self
            .start_dial_with_condition(peer_id, relay_addresses, PeerCondition::NotDialing)
            .await
        {
            Ok(()) => {}
            Err(ConnectError::DialFailed(DialError::DialPeerConditionFalse(
                PeerCondition::NotDialing,
            ))) => {
                log::debug!(
                    "Relay peer {peer_id} already has an in-flight dial; waiting for TCP carrier"
                );
            }
            Err(error) => return Err(error),
        }

        let deadline = tokio::time::Instant::now() + DIRECT_DIAL_TIMEOUT;
        loop {
            if self.relay_tcp_active(peer_id) {
                return Ok(());
            }

            if tokio::time::Instant::now() >= deadline {
                break;
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        Err(ConnectError::DialTimeout { peer_id })
    }

    fn is_dial_condition_false(error: &ConnectError) -> bool {
        matches!(
            error,
            ConnectError::DialFailed(DialError::DialPeerConditionFalse(_))
        )
    }

    async fn wait_for_existing_dial(
        &self,
        peer_id: PeerId,
        timeout: Duration,
    ) -> Result<(), ConnectError> {
        if self.wait_for_connection_count(peer_id, 1, timeout).await {
            return Ok(());
        }

        Err(ConnectError::DialTimeout { peer_id })
    }

    async fn connect_with_explicit_plan(&self, peer_id: PeerId) -> Result<(), ConnectError> {
        let min_connection_count = 1;
        let dial_plan = DialPlan::for_peer(&self.state, peer_id);
        let direct_addresses = dial_plan.direct_addresses();
        let mut direct_started = false;

        if direct_addresses.is_empty() {
            log::info!(
                "No direct dial candidates for peer {} ({}); preparing relay fallback",
                peer_id,
                dial_plan.direct_summary()
            );
        } else {
            if dial_plan.using_stale_direct_addresses() {
                log::info!(
                    "Using stale direct dial candidates for peer {} because no fresh/aging direct candidates are available: {}",
                    peer_id,
                    dial_plan.direct_summary()
                );
            } else {
                log::debug!(
                    "Dialing peer {} with explicit direct candidates: {}",
                    peer_id,
                    dial_plan.direct_summary()
                );
            }

            match self.start_dial(peer_id, direct_addresses).await {
                Ok(()) => {
                    direct_started = true;
                    if self
                        .wait_for_connection_count(
                            peer_id,
                            min_connection_count,
                            DIRECT_DIAL_FALLBACK_DELAY,
                        )
                        .await
                    {
                        return Ok(());
                    }
                }
                Err(error) if Self::is_dial_condition_false(&error) => {
                    log::debug!(
                        "Peer {} already has an in-flight dial; waiting for that dial to finish",
                        peer_id
                    );
                    return self
                        .wait_for_existing_dial(peer_id, DIRECT_DIAL_TIMEOUT)
                        .await;
                }
                Err(error) => {
                    log::info!(
                        "Direct dial to peer {} failed before start: {}; preparing relay fallback",
                        peer_id,
                        error
                    );
                }
            }
        }

        let relay_started = if self.relay_peers.is_empty() {
            false
        } else {
            log::info!(
                "Preparing relay fallback for peer {} after direct dial {}",
                peer_id,
                if direct_started {
                    "did not connect within fallback delay"
                } else {
                    "could not start"
                }
            );
            self.prepare_for_relay_fallback(peer_id).await;
            let relay_addresses = self.relay_peers.circuit_addresses_for_target(peer_id);
            match self.start_dial(peer_id, relay_addresses).await {
                Ok(()) => true,
                Err(error) => {
                    log::warn!(
                        "Relay fallback dial to peer {} failed before start: {}",
                        peer_id,
                        error
                    );
                    false
                }
            }
        };

        let wait_timeout = if relay_started {
            RELAY_DIAL_TIMEOUT
        } else if direct_started {
            DIRECT_DIAL_TIMEOUT
        } else {
            return Err(ConnectError::NoDialAddresses { peer_id });
        };

        if self
            .wait_for_connection_count(peer_id, min_connection_count, wait_timeout)
            .await
        {
            return Ok(());
        }

        Err(ConnectError::DialTimeout { peer_id })
    }

    async fn start_dial(
        &self,
        peer_id: PeerId,
        addresses: Vec<Multiaddr>,
    ) -> Result<(), ConnectError> {
        self.start_dial_with_condition(peer_id, addresses, PeerCondition::DisconnectedAndNotDialing)
            .await
    }

    async fn start_dial_with_condition(
        &self,
        peer_id: PeerId,
        addresses: Vec<Multiaddr>,
        condition: PeerCondition,
    ) -> Result<(), ConnectError> {
        if addresses.is_empty() {
            return Err(ConnectError::NoDialAddresses { peer_id });
        }

        let dial_result = self
            .invoke_swarm(move |swarm| {
                swarm.dial(
                    DialOpts::peer_id(peer_id)
                        .addresses(addresses)
                        .condition(condition)
                        .build(),
                )
            })
            .await;

        match dial_result {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => Err(ConnectError::DialFailed(error)),
            Err(error) => Err(ConnectError::SwarmInvocationFailed(error)),
        }
    }

    pub(super) fn relay_tcp_active(&self, peer_id: PeerId) -> bool {
        self.relay_peers
            .addresses_for_peer(peer_id)
            .is_some_and(|addresses| {
                addresses
                    .iter()
                    .any(|addr| self.state.relay_endpoint_active(addr))
            })
    }

    async fn wait_for_connection_count(
        &self,
        peer_id: PeerId,
        min_connection_count: usize,
        timeout: Duration,
    ) -> bool {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if self.peer_connection_count(peer_id) >= min_connection_count {
                return true;
            }

            if tokio::time::Instant::now() >= deadline {
                return false;
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    fn peer_connection_count(&self, peer_id: PeerId) -> usize {
        self.state
            .get_peer_connections(&peer_id)
            .map(|connections| connections.total_connections())
            .unwrap_or(0)
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

    pub(super) fn collect_selected_connections(&self, peer_id: PeerId) -> Vec<SelectedConnection> {
        let Some(peer_connections) = self.state.get_peer_connections(&peer_id) else {
            return Vec::new();
        };

        let mut selected = Vec::new();

        for connection in peer_connections.outbound() {
            selected.push(self.selected_connection(
                ConnectionDirection::Outbound,
                connection.connection_id(),
                connection.multiaddr().clone(),
            ));
        }

        for connection in peer_connections.inbound() {
            selected.push(self.selected_connection(
                ConnectionDirection::Inbound,
                connection.connection_id(),
                connection.multiaddr().clone(),
            ));
        }

        selected
    }

    fn selected_connection(
        &self,
        direction: ConnectionDirection,
        connection_id: ConnectionId,
        remote_addr: Multiaddr,
    ) -> SelectedConnection {
        let ping_info = self.state.connection_ping_info(&connection_id);
        let last_rtt = ping_info.and_then(|info| info.last_rtt);
        let active_stream_count = self
            .state
            .active_streams_by_connection(&connection_id)
            .len();
        let established_at = self.state.connection_established_at(&connection_id);

        SelectedConnection {
            connection_id,
            direction,
            is_relay: is_circuit_addr(&remote_addr),
            remote_addr,
            last_rtt,
            active_stream_count,
            established_at,
        }
    }
}
