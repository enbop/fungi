// Relay management policy for fungi:
//
// Relay reservation is UDP-first within each relay peer. If the UDP candidate
// does not become ready during the current reconciliation attempt, we fall back
// to TCP for availability. A healthy TCP fallback is not interrupted just to
// upgrade back to UDP; the next reconcile starts from UDP again.
use super::SwarmControl;
use crate::{State, behaviours::relay_refresh};
use anyhow::{Result, bail};
use libp2p::{
    Multiaddr, PeerId,
    multiaddr::Protocol,
    relay,
    swarm::{
        ConnectionId, DialError,
        dial_opts::{DialOpts, PeerCondition},
    },
};
use parking_lot::Mutex;
use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::time::Instant;

const RELAY_RETRY_FAST_ATTEMPTS: u32 = 4;
const RELAY_RETRY_BASE_DELAY_MS: u64 = 500;
const RELAY_RETRY_MAX_DELAY: Duration = Duration::from_secs(60);
const RELAY_HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(600);
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
    peer_id: Option<PeerId>,
    transport_kind: Option<RelayTransportKind>,
}

impl RelayEndpoint {
    pub(super) fn new(addr: Multiaddr) -> Self {
        Self {
            peer_id: peer_id_from_addr(&addr),
            transport_kind: relay_transport_kind(&addr),
            addr,
        }
    }

    pub(super) fn addr(&self) -> &Multiaddr {
        &self.addr
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
pub(super) struct RelayPeerGroup {
    peer_id: PeerId,
    endpoints: Arc<Vec<RelayEndpoint>>,
    task_flag: Arc<AtomicBool>,
}

impl RelayPeerGroup {
    fn new(peer_id: PeerId, endpoints: Vec<RelayEndpoint>) -> Self {
        Self {
            peer_id,
            endpoints: Arc::new(endpoints),
            task_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    pub(super) fn peer_id(&self) -> PeerId {
        self.peer_id
    }

    pub(super) fn endpoints(&self) -> std::slice::Iter<'_, RelayEndpoint> {
        self.endpoints.iter()
    }

    fn task_flag(&self) -> Arc<AtomicBool> {
        self.task_flag.clone()
    }

    fn contains_transport(&self, transport_kind: RelayTransportKind) -> bool {
        self.endpoints()
            .any(|endpoint| endpoint.transport_kind() == Some(transport_kind))
    }

    fn active_endpoint(&self, state: &State) -> Option<RelayEndpoint> {
        self.endpoints()
            .find(|endpoint| state.relay_endpoint_ready(endpoint.addr()))
            .cloned()
    }
}

#[derive(Clone)]
pub(super) struct RelayPeers {
    groups: Arc<Vec<RelayPeerGroup>>,
    all_endpoints: Arc<Vec<RelayEndpoint>>,
    peer_ids: Arc<Vec<PeerId>>,
}

#[derive(Debug)]
pub(super) struct RelayUdpRefreshTarget {
    pub(super) peer_id: PeerId,
    pub(super) addresses: Vec<Multiaddr>,
}

#[derive(Debug, Default)]
pub(super) struct RelayUdpRefreshPlan {
    pub(super) targets: Vec<RelayUdpRefreshTarget>,
    pub(super) skipped_circuit: usize,
    pub(super) skipped_missing_peer: usize,
    pub(super) skipped_duplicate_addr: usize,
    pub(super) preferred_relay_matched: bool,
}

impl RelayPeers {
    pub(super) fn new(relay_addresses: Vec<Multiaddr>) -> Self {
        let all_endpoints = relay_addresses
            .into_iter()
            .map(RelayEndpoint::new)
            .collect::<Vec<_>>();

        let mut peer_order = Vec::<PeerId>::new();
        let mut endpoints_by_peer = HashMap::<PeerId, Vec<RelayEndpoint>>::new();

        for relay_endpoint in &all_endpoints {
            if relay_endpoint.transport_kind().is_none() {
                continue;
            }
            let Some(peer_id) = relay_endpoint.peer_id() else {
                continue;
            };

            endpoints_by_peer.entry(peer_id).or_insert_with(|| {
                peer_order.push(peer_id);
                Vec::new()
            });
            endpoints_by_peer
                .get_mut(&peer_id)
                .expect("relay peer entry was just inserted")
                .push(relay_endpoint.clone());
        }

        let groups = peer_order
            .iter()
            .filter_map(|peer_id| {
                let endpoints = endpoints_by_peer.remove(peer_id)?;
                let mut udp_endpoints = endpoints
                    .iter()
                    .filter(|endpoint| endpoint.transport_kind() == Some(RelayTransportKind::Udp))
                    .cloned()
                    .collect::<Vec<_>>();
                udp_endpoints.extend(
                    endpoints.into_iter().filter(|endpoint| {
                        endpoint.transport_kind() == Some(RelayTransportKind::Tcp)
                    }),
                );
                let endpoints = udp_endpoints;
                Some(RelayPeerGroup::new(*peer_id, endpoints))
            })
            .collect::<Vec<_>>();

        Self {
            all_endpoints: Arc::new(all_endpoints),
            groups: Arc::new(groups),
            peer_ids: Arc::new(peer_order),
        }
    }

    pub(super) fn register_with_state(&self, state: &State) {
        for relay_endpoint in self.all_endpoints() {
            state.register_relay_endpoint(relay_endpoint.addr().clone());
        }
    }

    pub(super) fn groups(&self) -> std::slice::Iter<'_, RelayPeerGroup> {
        self.groups.iter()
    }

    pub(super) fn all_endpoints(&self) -> std::slice::Iter<'_, RelayEndpoint> {
        self.all_endpoints.iter()
    }

    pub(super) fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    pub(super) fn peer_ids(&self) -> &[PeerId] {
        self.peer_ids.as_slice()
    }

    pub(super) fn group_for_peer(&self, relay_peer_id: PeerId) -> Option<RelayPeerGroup> {
        self.groups()
            .find(|group| group.peer_id() == relay_peer_id)
            .cloned()
    }

    pub(super) fn group_for_endpoint(&self, endpoint: &RelayEndpoint) -> Option<RelayPeerGroup> {
        endpoint
            .peer_id()
            .and_then(|peer_id| self.group_for_peer(peer_id))
    }

    pub(super) fn groups_for_transport(
        &self,
        transport_kind: RelayTransportKind,
    ) -> Vec<RelayPeerGroup> {
        self.groups()
            .filter(|group| group.contains_transport(transport_kind))
            .cloned()
            .collect()
    }

    pub(super) fn circuit_addresses_for_target(
        &self,
        target_peer: PeerId,
        state: &State,
    ) -> Vec<Multiaddr> {
        let mut relays = Vec::<Multiaddr>::new();
        let mut seen = HashSet::<String>::new();

        for group in self.groups() {
            if let Some(active_endpoint) = group.active_endpoint(state) {
                push_relay_addr_once(&mut relays, &mut seen, active_endpoint.addr().clone());
            }
        }

        for group in self.groups() {
            for endpoint in group.endpoints() {
                push_relay_addr_once(&mut relays, &mut seen, endpoint.addr().clone());
            }
        }

        relays
            .into_iter()
            .map(|relay_addr| peer_addr_with_relay(target_peer, relay_addr))
            .collect()
    }

    pub(super) fn udp_refresh_plan(&self, preferred_relay: Option<PeerId>) -> RelayUdpRefreshPlan {
        // The refresh plan remains UDP-only because this path refreshes observed
        // UDP addresses for direct QUIC hole punching.
        let mut plan = RelayUdpRefreshPlan::default();
        let mut seen_addrs = HashSet::<String>::new();
        let mut addresses_by_peer = HashMap::<PeerId, Vec<Multiaddr>>::new();
        let mut peer_order = Vec::<PeerId>::new();

        for relay_endpoint in self.all_endpoints() {
            if relay_endpoint.transport_kind() != Some(RelayTransportKind::Udp) {
                continue;
            }
            let addr = relay_endpoint.addr();
            if is_circuit_addr(addr) {
                plan.skipped_circuit += 1;
                log::debug!(
                    "Skipping relay UDP refresh target {} because it is a relayed address",
                    addr
                );
                continue;
            }

            let Some(peer_id) = relay_endpoint.peer_id() else {
                plan.skipped_missing_peer += 1;
                log::debug!(
                    "Skipping relay UDP refresh target {} because it has no relay peer id",
                    addr
                );
                continue;
            };

            if !seen_addrs.insert(addr.to_string()) {
                plan.skipped_duplicate_addr += 1;
                log::debug!(
                    "Skipping duplicate relay UDP refresh target {} for peer {}",
                    addr,
                    peer_id
                );
                continue;
            }

            let entry = addresses_by_peer.entry(peer_id).or_insert_with(|| {
                peer_order.push(peer_id);
                Vec::new()
            });
            entry.push(addr.clone());
        }

        if let Some(preferred_relay) = preferred_relay
            && let Some(position) = peer_order
                .iter()
                .position(|peer_id| *peer_id == preferred_relay)
        {
            plan.preferred_relay_matched = true;
            let peer_id = peer_order.remove(position);
            peer_order.insert(0, peer_id);
        }

        plan.targets = peer_order
            .into_iter()
            .filter_map(|peer_id| {
                addresses_by_peer
                    .remove(&peer_id)
                    .map(|addresses| RelayUdpRefreshTarget { peer_id, addresses })
            })
            .collect();

        plan
    }
}

fn push_relay_addr_once(
    relays: &mut Vec<Multiaddr>,
    seen: &mut HashSet<String>,
    relay_addr: Multiaddr,
) {
    if seen.insert(relay_addr.to_string()) {
        relays.push(relay_addr);
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
    // TODO add throttling for each peer id
    async fn refresh_external_addresses_via_relays(&self, preferred_relay: Option<PeerId>) -> bool {
        let plan = self.relay_peers.udp_refresh_plan(preferred_relay);
        let target_addr_count = plan
            .targets
            .iter()
            .map(|target| target.addresses.len())
            .sum::<usize>();

        if plan.targets.is_empty() {
            log::debug!(
                "No relay UDP refresh targets available (preferred_relay={:?}, skipped_circuit={}, skipped_missing_peer={}, skipped_duplicate_addr={})",
                preferred_relay,
                plan.skipped_circuit,
                plan.skipped_missing_peer,
                plan.skipped_duplicate_addr
            );
            return false;
        }

        log::info!(
            "Starting relay UDP external address refresh via {} peer(s), {} address(es) (preferred_relay={:?}, preferred_matched={}, skipped_circuit={}, skipped_missing_peer={}, skipped_duplicate_addr={})",
            plan.targets.len(),
            target_addr_count,
            preferred_relay,
            plan.preferred_relay_matched,
            plan.skipped_circuit,
            plan.skipped_missing_peer,
            plan.skipped_duplicate_addr
        );

        let mut refresh_started = false;
        let mut skipped_without_active_udp_reservation = 0usize;

        for target in plan.targets {
            let relay_peer = target.peer_id;
            let address_count = target.addresses.len();
            let address_summary = summarize_multiaddrs(&target.addresses);
            let addresses = target.addresses;

            let active_endpoint = self
                .relay_peers
                .group_for_peer(relay_peer)
                .and_then(|group| group.active_endpoint(self.state()))
                .map(|endpoint| endpoint.addr().clone());
            if active_endpoint.as_ref().and_then(relay_transport_kind)
                != Some(RelayTransportKind::Udp)
            {
                skipped_without_active_udp_reservation += 1;
                log::debug!(
                    "Skipping relay UDP refresh dial to {} because UDP is not the active relay reservation endpoint; active_endpoint={:?}, candidate address(es): {}",
                    relay_peer,
                    active_endpoint,
                    address_summary
                );
                continue;
            }

            let dial_result = self
                .invoke_swarm(move |swarm| {
                    let dial_opts = DialOpts::peer_id(relay_peer)
                        .addresses(addresses)
                        .condition(PeerCondition::NotDialing)
                        .build();
                    swarm.dial(dial_opts)
                })
                .await;

            match dial_result {
                Ok(Ok(())) => {
                    refresh_started = true;
                    log::debug!(
                        "Started relay UDP refresh dial to {} using {} address(es): {}",
                        relay_peer,
                        address_count,
                        address_summary
                    );
                }
                Ok(Err(error)) => {
                    log::debug!(
                        "Relay UDP refresh dial to {} failed before start using {} address(es): {} ({})",
                        relay_peer,
                        address_count,
                        address_summary,
                        error
                    );
                }
                Err(error) => {
                    log::debug!(
                        "Failed to invoke relay UDP refresh dial to {} using {} address(es): {} ({})",
                        relay_peer,
                        address_count,
                        address_summary,
                        error
                    );
                }
            }
        }

        if skipped_without_active_udp_reservation > 0 {
            log::debug!(
                "Skipped {} relay UDP refresh target peer(s) because UDP was not the active relay reservation endpoint",
                skipped_without_active_udp_reservation
            );
        }

        refresh_started
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

        for group in swarm_control.relay_peers.groups() {
            let mut listener_states = Vec::new();
            for relay_endpoint in group.endpoints() {
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

                swarm_control
                    .state()
                    .record_relay_listener_check(relay_endpoint.addr(), listener_registered);
                listener_states.push((relay_endpoint.clone(), listener_registered));
            }

            if swarm_control.state().relay_peer_ready(group.peer_id()) {
                continue;
            }

            for (relay_endpoint, listener_registered) in listener_states {
                let action = if listener_registered {
                    crate::RelayManagementAction::DirectConnectionMissingReconcile
                } else {
                    crate::RelayManagementAction::ListenerMissingReconcile
                };
                swarm_control
                    .state()
                    .record_relay_management_action(relay_endpoint.addr(), action);
            }
            log::info!(
                "Relay audit re-establishing reservation for peer {} because no relay endpoint is ready",
                group.peer_id(),
            );
            spawn_relay_reconcile_task(&swarm_control, group);
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

    for group in swarm_control
        .relay_peers
        .groups_for_transport(transport_kind)
    {
        spawn_relay_reconcile_task(swarm_control, &group);
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
        .all_endpoints()
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
    if relay_endpoint
        .peer_id()
        .is_some_and(|relay_peer_id| swarm_control.state().relay_peer_ready(relay_peer_id))
    {
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
    if let Some(group) = swarm_control.relay_peers.group_for_endpoint(relay_endpoint) {
        spawn_relay_reconcile_task(swarm_control, &group);
    }
}

fn spawn_relay_reconcile_task(swarm_control: &SwarmControl, group: &RelayPeerGroup) {
    let Some(_guard) = TaskGuard::try_acquire(group.task_flag()) else {
        return;
    };

    for relay_endpoint in group.endpoints() {
        swarm_control
            .state()
            .set_relay_task_running(relay_endpoint.addr(), true);
        swarm_control.state().record_relay_management_action(
            relay_endpoint.addr(),
            crate::RelayManagementAction::ReconcileTaskStarted,
        );
    }

    let swarm_control_cl = swarm_control.clone();
    let group_cl = group.clone();

    tokio::spawn(async move {
        let _guard = _guard;

        let mut attempt = 1u32;
        loop {
            match ensure_relay_peer_reservation(&swarm_control_cl, &group_cl).await {
                Ok(ready_endpoint) => {
                    for relay_endpoint in group_cl.endpoints() {
                        swarm_control_cl
                            .state()
                            .set_relay_task_running(relay_endpoint.addr(), false);
                    }
                    swarm_control_cl.state().record_relay_management_action(
                        ready_endpoint.addr(),
                        crate::RelayManagementAction::ReconcileTaskSucceeded,
                    );
                    log::info!(
                        "Successfully connected to relay {} via {} on attempt {attempt}",
                        group_cl.peer_id(),
                        ready_endpoint.addr()
                    );
                    return;
                }
                Err(error) => {
                    let delay = relay_retry_delay(attempt);
                    for relay_endpoint in group_cl.endpoints() {
                        swarm_control_cl.state().record_relay_management_error(
                            relay_endpoint.addr(),
                            error.to_string(),
                        );
                    }
                    if attempt <= RELAY_RETRY_FAST_ATTEMPTS {
                        log::warn!(
                            "Failed to connect to relay peer {} on attempt {attempt}: {error}; retrying in {delay:?}",
                            group_cl.peer_id()
                        );
                    } else {
                        log::warn!(
                            "Relay peer {} is still unavailable on attempt {attempt}: {error}; continuing background retry in {delay:?}",
                            group_cl.peer_id()
                        );
                    }

                    tokio::time::sleep(delay).await;
                    attempt = attempt.saturating_add(1);
                }
            }
        }
    });
}

pub(super) fn relay_retry_delay(attempt: u32) -> Duration {
    if attempt > RELAY_RETRY_FAST_ATTEMPTS {
        return RELAY_RETRY_MAX_DELAY;
    }

    let shift = attempt.saturating_sub(1);
    let delay_ms = RELAY_RETRY_BASE_DELAY_MS.saturating_mul(1u64 << shift);
    Duration::from_millis(delay_ms).min(RELAY_RETRY_MAX_DELAY)
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

    for relay_endpoint in swarm_control.relay_peers.all_endpoints() {
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
            return;
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

    for relay_endpoint in swarm_control.relay_peers.all_endpoints() {
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
            return;
        }
    }
}

async fn ensure_relay_peer_reservation(
    swarm_control: &SwarmControl,
    group: &RelayPeerGroup,
) -> Result<RelayEndpoint> {
    if let Some(active_endpoint) = group
        .endpoints()
        .find(|endpoint| swarm_control.state().relay_endpoint_ready(endpoint.addr()))
        .cloned()
    {
        return Ok(active_endpoint);
    }

    let mut last_error = None;
    for endpoint in group.endpoints() {
        match try_relay_endpoint_reservation(swarm_control, endpoint).await {
            Ok(()) => return Ok(endpoint.clone()),
            Err(error) => {
                swarm_control
                    .state()
                    .record_relay_management_error(endpoint.addr(), error.to_string());
                log::debug!(
                    "Relay endpoint {} failed during UDP-first reservation attempt: {}",
                    endpoint.addr(),
                    error
                );
                last_error = Some(error);
            }
        }
    }

    match last_error {
        Some(error) => Err(error),
        None => bail!(
            "Relay peer {} has no usable relay endpoints",
            group.peer_id()
        ),
    }
}

async fn try_relay_endpoint_reservation(
    swarm_control: &SwarmControl,
    relay_endpoint: &RelayEndpoint,
) -> Result<()> {
    let relay_peer = relay_endpoint
        .peer_id()
        .ok_or_else(|| anyhow::anyhow!("Invalid relay address"))?;
    let relay_addr = relay_endpoint.addr().clone();

    close_other_relay_connections_for_peer(swarm_control, relay_peer, relay_endpoint.addr()).await;
    ensure_relay_endpoint_carrier(swarm_control, relay_peer, relay_addr.clone()).await?;

    if relay_listener_registered(swarm_control, relay_endpoint).await? {
        swarm_control
            .state()
            .record_relay_listener_check(relay_endpoint.addr(), true);
        wait_relay_endpoint_ready(swarm_control, relay_endpoint).await?;
        return Ok(());
    }

    println!("Listening on relay address: {relay_addr:?}");
    swarm_control
        .invoke_swarm(move |swarm| swarm.listen_on(relay_addr.with(Protocol::P2pCircuit)))
        .await??;

    wait_relay_endpoint_ready(swarm_control, relay_endpoint).await
}

async fn close_other_relay_connections_for_peer(
    swarm_control: &SwarmControl,
    relay_peer: PeerId,
    active_relay_addr: &Multiaddr,
) {
    let connection_ids = swarm_control
        .state()
        .list_relay_endpoint_statuses()
        .into_iter()
        .filter(|status| {
            status.relay_peer_id == Some(relay_peer) && status.relay_addr != *active_relay_addr
        })
        .filter_map(|status| status.current_direct_connection_id)
        .collect::<Vec<_>>();

    if connection_ids.is_empty() {
        return;
    }

    for connection_id in &connection_ids {
        match swarm_control.close_connection(*connection_id).await {
            Ok(true) => {
                log::debug!(
                    "Closing non-selected relay carrier connection {} for peer {} before reservation on {}",
                    connection_id,
                    relay_peer,
                    active_relay_addr
                );
            }
            Ok(false) => {
                log::debug!(
                    "Non-selected relay carrier connection {} for peer {} was already closing or absent",
                    connection_id,
                    relay_peer
                );
            }
            Err(error) => {
                log::debug!(
                    "Failed to close non-selected relay carrier connection {} for peer {} before reservation on {}: {}",
                    connection_id,
                    relay_peer,
                    active_relay_addr,
                    error
                );
            }
        }
    }

    tokio::time::sleep(Duration::from_millis(100)).await;
}

async fn ensure_relay_endpoint_carrier(
    swarm_control: &SwarmControl,
    relay_peer: PeerId,
    relay_addr: Multiaddr,
) -> Result<()> {
    if swarm_control.state().relay_endpoint_active(&relay_addr) {
        return Ok(());
    }

    let dial_result = swarm_control
        .invoke_swarm({
            let relay_addr = relay_addr.clone();
            move |swarm| {
                swarm.dial(
                    DialOpts::peer_id(relay_peer)
                        .addresses(vec![relay_addr])
                        .condition(PeerCondition::NotDialing)
                        .build(),
                )
            }
        })
        .await?;

    match dial_result {
        Ok(()) => {}
        Err(DialError::DialPeerConditionFalse(PeerCondition::NotDialing)) => {
            log::debug!(
                "Relay peer {relay_peer} already has an in-flight dial; waiting for carrier {}",
                relay_addr
            );
        }
        Err(error) => bail!("Failed to dial relay {relay_peer} via {relay_addr}: {error}"),
    }

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if swarm_control.state().relay_endpoint_active(&relay_addr) {
            return Ok(());
        }

        if Instant::now() >= deadline {
            bail!("Timed out waiting for relay carrier {relay_peer} via {relay_addr}");
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn wait_relay_endpoint_ready(
    swarm_control: &SwarmControl,
    relay_endpoint: &RelayEndpoint,
) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let listener_registered = relay_listener_registered(swarm_control, relay_endpoint).await?;
        swarm_control
            .state()
            .record_relay_listener_check(relay_endpoint.addr(), listener_registered);

        if swarm_control
            .state()
            .relay_endpoint_ready(relay_endpoint.addr())
        {
            return Ok(());
        }

        if Instant::now() >= deadline {
            bail!(
                "Timed out waiting for relay endpoint to become ready: {}",
                relay_endpoint.addr()
            );
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

pub fn get_default_relay_addrs() -> Vec<Multiaddr> {
    vec![
        "/ip4/160.16.206.21/udp/30001/quic-v1/p2p/16Uiu2HAmGXFS6aYsKKYRkEDo1tNigZKN8TAYrsfSnEdC5sZLNkiE"
            .parse()
            .unwrap(),
        "/ip4/160.16.206.21/tcp/30001/p2p/16Uiu2HAmGXFS6aYsKKYRkEDo1tNigZKN8TAYrsfSnEdC5sZLNkiE"
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
