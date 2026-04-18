use super::{ConnectionSelectionStrategy, SwarmControl};
use crate::{State, behaviours::relay_refresh};
use anyhow::{Result, bail};
use fungi_util::protocols::FUNGI_RELAY_HANDSHAKE_PROTOCOL;
use libp2p::{
    Multiaddr, PeerId,
    futures::AsyncReadExt,
    multiaddr::Protocol,
    relay,
    swarm::{
        ConnectionId,
        dial_opts::{DialOpts, PeerCondition},
    },
};
use parking_lot::Mutex;
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::time::Instant;

const RELAY_RETRY_MAX_ATTEMPTS: u32 = 4;
const RELAY_RETRY_BASE_DELAY_MS: u64 = 500;
const RELAY_HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(600);
const RELAY_PREPARE_REFRESH_GRACE_WINDOW: Duration = Duration::from_millis(800);
const RELAY_REFRESH_MIN_INTERVAL: Duration = Duration::from_secs(2);

struct TaskGuard {
    flag: Arc<AtomicBool>,
}

impl TaskGuard {
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

#[derive(Clone, Default)]
pub(super) struct RefreshThrottle {
    last_refresh_at: Arc<Mutex<Option<Instant>>>,
}

impl RefreshThrottle {
    pub(super) fn is_rate_limited(&self) -> bool {
        let now = Instant::now();
        let mut last_refresh_at = self.last_refresh_at.lock();
        if let Some(last_refresh_at_value) = *last_refresh_at
            && now.duration_since(last_refresh_at_value) < RELAY_REFRESH_MIN_INTERVAL
        {
            return true;
        }

        *last_refresh_at = Some(now);
        false
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RelayTransportKind {
    Tcp,
    Udp,
}

#[derive(Clone)]
pub(super) struct RelayEndpoint {
    addr: Multiaddr,
    task_flag: Arc<AtomicBool>,
    peer_id: Option<PeerId>,
    transport_kind: Option<RelayTransportKind>,
}

impl RelayEndpoint {
    pub(super) fn new(addr: Multiaddr) -> Self {
        Self {
            peer_id: peer_id_from_addr(&addr),
            transport_kind: relay_transport_kind(&addr),
            addr,
            task_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    pub(super) fn addr(&self) -> &Multiaddr {
        &self.addr
    }

    fn task_flag(&self) -> &Arc<AtomicBool> {
        &self.task_flag
    }

    pub(super) fn peer_id(&self) -> Option<PeerId> {
        self.peer_id
    }

    fn listener_prefix(&self) -> Multiaddr {
        self.addr.clone().with(Protocol::P2pCircuit)
    }

    pub(super) fn matches_listener(&self, listener_addr: &Multiaddr) -> bool {
        multiaddr_starts_with(listener_addr, &self.listener_prefix())
    }

    pub(super) fn transport_kind(&self) -> Option<RelayTransportKind> {
        self.transport_kind
    }

    pub(super) fn matches_transport(&self, remote_addr: &Multiaddr) -> bool {
        self.transport_kind()
            .zip(relay_transport_kind(remote_addr))
            .is_some_and(|(left, right)| left == right)
    }
}

#[derive(Clone)]
pub(super) struct RelayPeers {
    endpoints: Arc<Vec<RelayEndpoint>>,
    addresses_by_peer: Arc<HashMap<PeerId, Vec<Multiaddr>>>,
    peer_ids: Arc<Vec<PeerId>>,
}

impl RelayPeers {
    pub(super) fn new(relay_addresses: Vec<Multiaddr>) -> Self {
        let endpoints = relay_addresses
            .into_iter()
            .map(RelayEndpoint::new)
            .collect::<Vec<_>>();
        let mut addresses_by_peer = HashMap::<PeerId, Vec<Multiaddr>>::new();
        let mut peer_ids = Vec::new();

        for relay_endpoint in &endpoints {
            let Some(peer_id) = relay_endpoint.peer_id() else {
                continue;
            };

            let entry = addresses_by_peer.entry(peer_id).or_insert_with(|| {
                peer_ids.push(peer_id);
                Vec::new()
            });
            entry.push(relay_endpoint.addr().clone());
        }

        Self {
            endpoints: Arc::new(endpoints),
            addresses_by_peer: Arc::new(addresses_by_peer),
            peer_ids: Arc::new(peer_ids),
        }
    }

    pub(super) fn register_with_state(&self, state: &State) {
        for relay_endpoint in self.iter() {
            state.register_relay_endpoint(relay_endpoint.addr().clone());
        }
    }

    pub(super) fn iter(&self) -> std::slice::Iter<'_, RelayEndpoint> {
        self.endpoints.iter()
    }

    pub(super) fn is_empty(&self) -> bool {
        self.endpoints.is_empty()
    }

    pub(super) fn len(&self) -> usize {
        self.endpoints.len()
    }

    pub(super) fn peer_ids(&self) -> &[PeerId] {
        self.peer_ids.as_slice()
    }

    pub(super) fn addresses_for_peer(&self, peer_id: PeerId) -> Option<Vec<Multiaddr>> {
        self.addresses_by_peer.get(&peer_id).cloned()
    }

    pub(super) fn circuit_addresses_for_target(&self, target_peer: PeerId) -> Vec<Multiaddr> {
        self.iter()
            .map(|relay_endpoint| peer_addr_with_relay(target_peer, relay_endpoint.addr().clone()))
            .collect()
    }
}

pub(super) fn relay_transport_kind(addr: &Multiaddr) -> Option<RelayTransportKind> {
    for protocol in addr.iter() {
        match protocol {
            Protocol::Tcp(_) => return Some(RelayTransportKind::Tcp),
            Protocol::Udp(_) => return Some(RelayTransportKind::Udp),
            _ => {}
        }
    }

    None
}

pub(super) fn multiaddr_starts_with(addr: &Multiaddr, prefix: &Multiaddr) -> bool {
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

pub(super) fn is_circuit_addr(addr: &Multiaddr) -> bool {
    addr.iter()
        .any(|protocol| matches!(protocol, Protocol::P2pCircuit))
}

impl SwarmControl {
    pub(super) async fn force_redial_relay_peer(&self, relay_peer: PeerId) -> Result<()> {
        let Some(relay_addresses) = self.relay_peers.addresses_for_peer(relay_peer) else {
            bail!("Relay peer {relay_peer} is not present in the configured relay list");
        };

        if self
            .state()
            .dial_callback()
            .lock()
            .contains_key(&relay_peer)
        {
            log::debug!(
                "Relay peer {} is already dialing; skipping refresh redial",
                relay_peer
            );
            return Ok(());
        }

        let (completer, result) = async_result::AsyncResult::new_split::<
            std::result::Result<(), libp2p::swarm::DialError>,
        >();
        self.state()
            .dial_callback()
            .lock()
            .insert(relay_peer, completer);

        let dial_result = self
            .invoke_swarm(move |swarm| {
                let dial_opts = DialOpts::peer_id(relay_peer)
                    .addresses(relay_addresses)
                    .condition(PeerCondition::Always)
                    .build();
                swarm.dial(dial_opts)
            })
            .await;

        match dial_result {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                self.state().dial_callback().lock().remove(&relay_peer);
                bail!("Failed to redial relay peer {relay_peer}: {error}");
            }
            Err(error) => {
                self.state().dial_callback().lock().remove(&relay_peer);
                bail!("Failed to redial relay peer {relay_peer}: {error}");
            }
        }

        result
            .await
            .map_err(|_| anyhow::anyhow!("Relay refresh dial to {relay_peer} was cancelled"))??;

        Ok(())
    }

    async fn refresh_external_addresses_via_relays(&self, preferred_relay: Option<PeerId>) -> bool {
        let _ = preferred_relay;

        // TODO: Actively dial every deduplicated relay UDP address here to refresh observed
        // external addresses. The new one-way relay-refresh protocol is wired first, while the
        // concrete UDP refresh strategy stays intentionally simple and deferred.
        false
    }

    pub(super) async fn trigger_external_address_refresh(
        &self,
        preferred_relay: Option<PeerId>,
    ) -> bool {
        if self.refresh_throttle.is_rate_limited() {
            return false;
        }

        self.refresh_external_addresses_via_relays(preferred_relay)
            .await
    }

    async fn send_prepare_refresh_notification(
        &self,
        relay_peer: PeerId,
        target_peer: PeerId,
    ) -> Result<()> {
        self.invoke_swarm(move |swarm| {
            swarm
                .behaviour_mut()
                .send_relay_refresh(&relay_peer, target_peer)
        })
        .await?;

        Ok(())
    }

    pub(super) async fn prepare_for_relay_fallback(&self, target_peer: PeerId) {
        let local_refresh_started = self.trigger_external_address_refresh(None).await;
        let mut remote_refresh_requested = false;

        for relay_peer in self.relay_peers.peer_ids().iter().copied() {
            let relay_connected = match self
                .invoke_swarm(move |swarm| swarm.is_connected(&relay_peer))
                .await
            {
                Ok(value) => value,
                Err(error) => {
                    log::debug!(
                        "Failed to inspect relay connection state for {} before prepare-refresh: {}",
                        relay_peer,
                        error
                    );
                    false
                }
            };

            if !relay_connected && self.force_redial_relay_peer(relay_peer).await.is_err() {
                continue;
            }

            match self
                .send_prepare_refresh_notification(relay_peer, target_peer)
                .await
            {
                Ok(()) => {
                    remote_refresh_requested = true;
                    break;
                }
                Err(error) => {
                    log::debug!(
                        "Relay {} prepare-refresh notify for target {} failed: {}",
                        relay_peer,
                        target_peer,
                        error
                    );
                }
            }
        }

        if local_refresh_started || remote_refresh_requested {
            tokio::time::sleep(RELAY_PREPARE_REFRESH_GRACE_WINDOW).await;
        }
    }
}

pub(super) fn handle_relay_behaviour_event(
    swarm_control: &SwarmControl,
    event: relay::client::Event,
) {
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

pub(super) fn handle_relay_refresh_behaviour_event(
    swarm_control: &SwarmControl,
    event: relay_refresh::Event,
) {
    log::info!(
        "Received relay refresh notification from {} for requester {}",
        event.peer,
        event.announced_peer_id
    );

    let swarm_control = swarm_control.clone();
    tokio::spawn(async move {
        if !swarm_control
            .trigger_external_address_refresh(Some(event.peer))
            .await
        {
            log::debug!(
                "Relay refresh notification from {} did not trigger a refresh",
                event.peer
            );
        }
    });
}

pub(super) async fn relay_management_loop(swarm_control: SwarmControl) {
    loop {
        tokio::time::sleep(RELAY_HEALTH_CHECK_INTERVAL).await;

        if swarm_control.relay_peers.is_empty() {
            continue;
        }

        let mut endpoint_health = Vec::with_capacity(swarm_control.relay_peers.len());

        for relay_endpoint in swarm_control.relay_peers.iter() {
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

pub(super) fn handle_new_listen_addr(swarm_control: &SwarmControl, new_addr: Multiaddr) {
    if is_circuit_addr(&new_addr) {
        record_matching_relay_listener_state(swarm_control, &new_addr, true);
        return;
    }

    if !is_public_listen_addr(&new_addr) {
        return;
    }

    let Some(transport_kind) = relay_transport_kind(&new_addr) else {
        return;
    };

    for relay_endpoint in swarm_control.relay_peers.iter() {
        if relay_endpoint.transport_kind() != Some(transport_kind) {
            continue;
        }
        spawn_relay_listen_task(swarm_control, relay_endpoint);
    }
}

pub(super) fn handle_expired_listen_addr(swarm_control: &SwarmControl, expired_addr: Multiaddr) {
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

pub(super) fn handle_listener_closed(
    swarm_control: &SwarmControl,
    addresses: Vec<Multiaddr>,
    reason: Result<(), std::io::Error>,
) {
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
        .relay_peers
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
                Err(error) => {
                    swarm_control_cl
                        .state()
                        .record_relay_management_error(&relay_addr_cl, error.to_string());
                    log::warn!(
                        "Failed to connect to relay {relay_addr_cl:?} on attempt {attempt}: {error}"
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

pub(super) fn record_relay_connection_established(
    swarm_control: &SwarmControl,
    peer_id: PeerId,
    connection_id: ConnectionId,
    remote_addr: &Multiaddr,
) {
    if is_circuit_addr(remote_addr) {
        return;
    }

    for relay_endpoint in swarm_control.relay_peers.iter() {
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
                transport_name(relay_endpoint.transport_kind()),
                connection_id,
                remote_addr
            );
        }
    }
}

pub(super) fn record_relay_connection_closed(
    swarm_control: &SwarmControl,
    peer_id: PeerId,
    connection_id: ConnectionId,
    remote_addr: &Multiaddr,
    cause: Option<String>,
) {
    if is_circuit_addr(remote_addr) {
        return;
    }

    for relay_endpoint in swarm_control.relay_peers.iter() {
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
                log::warn!(
                    "Relay carrier connection closed peer={} transport={} connection_id={} addr={} cause={}",
                    peer_id,
                    transport_name(relay_endpoint.transport_kind()),
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
    let relay_peer =
        peer_id_from_addr(&relay_addr).ok_or_else(|| anyhow::anyhow!("Invalid relay address"))?;

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
    let dial_result = swarm_control
        .invoke_swarm(move |swarm| -> std::result::Result<(), String> {
            if force_redial {
                let dial_opts = DialOpts::peer_id(relay_peer)
                    .addresses(vec![relay_addr.clone()])
                    .condition(PeerCondition::Always)
                    .build();
                return swarm.dial(dial_opts).map_err(|error| {
                    format!("Failed to force redial relay {relay_peer} via {relay_addr}: {error}")
                });
            }

            if !swarm.is_connected(&relay_peer) {
                swarm.dial(relay_addr.clone()).map_err(|error| {
                    format!("Failed to dial relay address {relay_peer} via {relay_addr}: {error}")
                })?;
            }

            Ok(())
        })
        .await?;

    dial_result.map_err(|error| anyhow::anyhow!(error))
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
        Err(error) => bail!("Handshake failed: {error:?}"),
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

fn is_public_listen_addr(addr: &Multiaddr) -> bool {
    match addr.iter().next() {
        Some(Protocol::Ip4(ip)) => {
            !ip.is_loopback() && !ip.is_broadcast() && !ip.is_multicast() && !ip.is_unspecified()
        }
        Some(Protocol::Ip6(ip)) => {
            !ip.is_loopback()
                && !ip.is_unicast_link_local()
                && !ip.is_unique_local()
                && !ip.is_multicast()
                && !ip.is_unspecified()
        }
        _ => false,
    }
}

fn peer_id_from_addr(addr: &Multiaddr) -> Option<PeerId> {
    addr.iter().find_map(|protocol| match protocol {
        Protocol::P2p(peer_id) => Some(peer_id),
        _ => None,
    })
}

fn transport_name(kind: Option<RelayTransportKind>) -> &'static str {
    match kind {
        Some(RelayTransportKind::Tcp) => "tcp",
        Some(RelayTransportKind::Udp) => "udp",
        None => "unknown",
    }
}
