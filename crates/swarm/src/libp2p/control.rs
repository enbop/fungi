use super::{
    ConnectionSelectionStrategy, SelectedConnection, TSwarm,
    relay::{RefreshThrottle, RelayPeers, is_circuit_addr},
};
use crate::{
    ConnectionDirection, State, StreamObservationHandle,
    ping::{PING_PROTOCOL, PingState, send_ping_with_timeout},
};
use anyhow::{Result, bail};
use async_result::{AsyncResult, Completer};
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
    #[error("Already dialing peer {peer_id}")]
    AlreadyDialing { peer_id: PeerId },
    #[error("Swarm invocation failed: {0}")]
    SwarmInvocationFailed(anyhow::Error),
    #[error("Connection cancelled")]
    Cancelled,
}

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
                        PING_PROTOCOL,
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
            if attempt == 1 {
                log::info!(
                    "Retrying stream open to peer {} after forced redial",
                    target_peer
                );
                if let Err(error) = self.connect_force_redial(target_peer).await {
                    log::warn!("Forced redial to peer {} failed: {}", target_peer, error);
                }
                tokio::time::sleep(Duration::from_millis(300)).await;
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
        self.connect_internal(peer_id, false).await
    }

    async fn connect_force_redial(&self, peer_id: PeerId) -> Result<(), ConnectError> {
        self.connect_internal(peer_id, true).await
    }

    async fn connect_internal(
        &self,
        peer_id: PeerId,
        force_redial_when_connected: bool,
    ) -> Result<(), ConnectError> {
        if !force_redial_when_connected {
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
        }

        if self.state.dial_callback().lock().contains_key(&peer_id) {
            log::warn!("Already dialing {peer_id}");
            return Err(ConnectError::AlreadyDialing { peer_id });
        }

        let (completer, result) = AsyncResult::new_split::<std::result::Result<(), DialError>>();
        self.state.dial_callback().lock().insert(peer_id, completer);

        let direct_dial_result = self
            .invoke_swarm(move |swarm| {
                if force_redial_when_connected {
                    log::info!("Force redialing peer {peer_id}");
                    let dial_opts = DialOpts::peer_id(peer_id)
                        .condition(PeerCondition::Always)
                        .build();
                    swarm.dial(dial_opts)
                } else {
                    log::debug!("Dialing peer {peer_id} directly");
                    swarm.dial(peer_id)
                }
            })
            .await;

        let relay_peers = self.relay_peers.clone();

        match direct_dial_result {
            Ok(Ok(())) => {}
            Ok(Err(DialError::NoAddresses)) if !relay_peers.is_empty() => {
                log::info!(
                    "No direct addresses for {peer_id}; preparing relay refresh before fallback dial"
                );
                self.prepare_for_relay_fallback(peer_id).await;

                let relay_addresses = relay_peers.circuit_addresses_for_target(peer_id);
                let relay_dial_result = self
                    .invoke_swarm(move |swarm| {
                        let mut dial_opts = DialOpts::peer_id(peer_id).addresses(relay_addresses);
                        if force_redial_when_connected {
                            dial_opts = dial_opts.condition(PeerCondition::Always);
                        }
                        swarm.dial(dial_opts.build())
                    })
                    .await;

                match relay_dial_result {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => {
                        self.state.dial_callback().lock().remove(&peer_id);
                        return Err(ConnectError::DialFailed(error));
                    }
                    Err(error) => {
                        self.state.dial_callback().lock().remove(&peer_id);
                        log::warn!("Failed to invoke swarm for relay dial to {peer_id}: {error:?}");
                        return Err(ConnectError::SwarmInvocationFailed(error));
                    }
                }
            }
            Ok(Err(error)) => {
                self.state.dial_callback().lock().remove(&peer_id);
                return Err(ConnectError::DialFailed(error));
            }
            Err(error) => {
                self.state.dial_callback().lock().remove(&peer_id);
                log::warn!("Failed to invoke swarm for dial: {error:?}");
                return Err(ConnectError::SwarmInvocationFailed(error));
            }
        }

        result.await.map_err(|_| ConnectError::Cancelled)??;
        Ok(())
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
