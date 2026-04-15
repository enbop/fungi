use libp2p::{Multiaddr, PeerId, multiaddr::Protocol, swarm::ConnectionId};
use std::{
    collections::HashMap,
    time::{Duration, SystemTime},
};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum AddressTransportKind {
    Tcp,
    Udp,
    Relayed,
    Other,
}

impl AddressTransportKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            AddressTransportKind::Tcp => "tcp",
            AddressTransportKind::Udp => "udp",
            AddressTransportKind::Relayed => "relayed",
            AddressTransportKind::Other => "other",
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ExternalAddressSource {
    SwarmCandidate,
    SwarmConfirmed,
    RelayReservation,
}

impl ExternalAddressSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExternalAddressSource::SwarmCandidate => "swarm-candidate",
            ExternalAddressSource::SwarmConfirmed => "swarm-confirmed",
            ExternalAddressSource::RelayReservation => "relay-reservation",
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum AddressFreshness {
    Fresh,
    Aging,
    Stale,
    Expired,
}

impl AddressFreshness {
    pub fn as_str(&self) -> &'static str {
        match self {
            AddressFreshness::Fresh => "fresh",
            AddressFreshness::Aging => "aging",
            AddressFreshness::Stale => "stale",
            AddressFreshness::Expired => "expired",
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum RelayManagementAction {
    ListenTaskStarted,
    ListenTaskSucceeded,
    ListenTaskExhausted,
    ListenerMissingReconcile,
    ReservationEstablished,
    ReservationRenewed,
    DirectConnectionClosed,
    DirectConnectionClosedAwaitingManagementLoop,
}

impl RelayManagementAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            RelayManagementAction::ListenTaskStarted => "listen-task-started",
            RelayManagementAction::ListenTaskSucceeded => "listen-task-succeeded",
            RelayManagementAction::ListenTaskExhausted => "listen-task-exhausted",
            RelayManagementAction::ListenerMissingReconcile => "listener-missing-reconcile",
            RelayManagementAction::ReservationEstablished => "reservation-established",
            RelayManagementAction::ReservationRenewed => "reservation-renewed",
            RelayManagementAction::DirectConnectionClosed => "direct-connection-closed",
            RelayManagementAction::DirectConnectionClosedAwaitingManagementLoop => {
                "direct-connection-closed-awaiting-management-loop"
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExternalAddressCandidateRecord {
    pub address: Multiaddr,
    pub transport_kind: AddressTransportKind,
    pub first_observed_at: SystemTime,
    pub last_observed_at: SystemTime,
    pub confirmed_at: Option<SystemTime>,
    pub expired_at: Option<SystemTime>,
    pub observation_count: u64,
    pub sources: Vec<ExternalAddressSource>,
}

impl ExternalAddressCandidateRecord {
    pub fn freshness(&self, now: SystemTime) -> AddressFreshness {
        if self.expired_at.is_some() {
            return AddressFreshness::Expired;
        }

        let age = now
            .duration_since(self.last_observed_at)
            .unwrap_or(Duration::ZERO);
        let (fresh_window, aging_window) = freshness_windows(self.transport_kind);

        if age <= fresh_window {
            AddressFreshness::Fresh
        } else if age <= aging_window {
            AddressFreshness::Aging
        } else {
            AddressFreshness::Stale
        }
    }

    pub fn recommend_refresh_before_dcutr(&self, now: SystemTime) -> bool {
        matches!(
            (self.transport_kind, self.freshness(now)),
            (AddressTransportKind::Udp, AddressFreshness::Aging)
                | (_, AddressFreshness::Stale)
                | (_, AddressFreshness::Expired)
        )
    }
}

#[derive(Debug, Clone)]
pub struct RelayEndpointStatusRecord {
    pub relay_addr: Multiaddr,
    pub relay_peer_id: Option<PeerId>,
    pub transport_kind: AddressTransportKind,
    pub listener_registered: bool,
    pub task_running: bool,
    pub current_direct_connection_id: Option<ConnectionId>,
    pub last_listener_seen_at: Option<SystemTime>,
    pub last_listener_missing_at: Option<SystemTime>,
    pub last_reservation_accepted_at: Option<SystemTime>,
    pub last_reservation_established_at: Option<SystemTime>,
    pub last_reservation_renewed_at: Option<SystemTime>,
    pub last_direct_connection_closed_at: Option<SystemTime>,
    /// Last reconciliation action taken by the relay manager for this endpoint.
    pub last_management_action: Option<RelayManagementAction>,
    /// Last reconciliation error observed for this endpoint.
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct RelayDirectConnectionSnapshot {
    pub transport_kind: AddressTransportKind,
    pub connection_id: ConnectionId,
}

/// Observability-oriented state for external address learning and relay runtime management.
///
/// This is intentionally separate from the peer/connection registry because the data has a
/// different lifecycle and is primarily used for decision making and diagnostics rather than
/// connection ownership tracking.
#[derive(Debug, Default)]
pub struct ConnectivityState {
    external_address_candidates: HashMap<Multiaddr, ExternalAddressCandidateRecord>,
    relay_endpoint_statuses: HashMap<Multiaddr, RelayEndpointStatusRecord>,
}

impl ConnectivityState {
    pub fn register_relay_endpoint(&mut self, relay_addr: Multiaddr) {
        let transport_kind = address_transport_kind(&relay_addr);
        let relay_peer_id = relay_peer_id(&relay_addr);

        self.relay_endpoint_statuses
            .entry(relay_addr.clone())
            .or_insert_with(|| RelayEndpointStatusRecord {
                relay_addr,
                relay_peer_id,
                transport_kind,
                listener_registered: false,
                task_running: false,
                current_direct_connection_id: None,
                last_listener_seen_at: None,
                last_listener_missing_at: None,
                last_reservation_accepted_at: None,
                last_reservation_established_at: None,
                last_reservation_renewed_at: None,
                last_direct_connection_closed_at: None,
                last_management_action: None,
                last_error: None,
            });
    }

    pub fn set_relay_task_running(&mut self, relay_addr: &Multiaddr, task_running: bool) {
        if let Some(status) = self.relay_endpoint_statuses.get_mut(relay_addr) {
            status.task_running = task_running;
        }
    }

    pub fn record_relay_listener_check(
        &mut self,
        relay_addr: &Multiaddr,
        listener_registered: bool,
    ) {
        if let Some(status) = self.relay_endpoint_statuses.get_mut(relay_addr) {
            status.listener_registered = listener_registered;
            let now = SystemTime::now();
            if listener_registered {
                status.last_listener_seen_at = Some(now);
            } else {
                status.last_listener_missing_at = Some(now);
            }
        }
    }

    pub fn record_relay_management_action(
        &mut self,
        relay_addr: &Multiaddr,
        action: RelayManagementAction,
    ) {
        if let Some(status) = self.relay_endpoint_statuses.get_mut(relay_addr) {
            status.last_management_action = Some(action);
        }
    }

    pub fn record_relay_management_error(
        &mut self,
        relay_addr: &Multiaddr,
        error: impl Into<String>,
    ) {
        if let Some(status) = self.relay_endpoint_statuses.get_mut(relay_addr) {
            status.last_error = Some(error.into());
        }
    }

    pub fn record_relay_reservation_accepted(
        &mut self,
        relay_peer_id: PeerId,
        change: RelayManagementAction,
        direct_connections: &[RelayDirectConnectionSnapshot],
    ) {
        let now = SystemTime::now();

        for status in self.relay_endpoint_statuses.values_mut() {
            if status.relay_peer_id == Some(relay_peer_id) {
                status.current_direct_connection_id = direct_connections
                    .iter()
                    .find(|snapshot| snapshot.transport_kind == status.transport_kind)
                    .map(|snapshot| snapshot.connection_id);
                status.last_reservation_accepted_at = Some(now);
                match change {
                    RelayManagementAction::ReservationEstablished => {
                        status.last_reservation_established_at = Some(now);
                    }
                    RelayManagementAction::ReservationRenewed => {
                        status.last_reservation_renewed_at = Some(now);
                    }
                    _ => {}
                }
                status.last_management_action = Some(change);
            }
        }
    }

    pub fn record_relay_connection_closed(
        &mut self,
        relay_peer_id: PeerId,
        connection_id: ConnectionId,
        remote_addr: &Multiaddr,
    ) {
        let remote_transport = address_transport_kind(remote_addr);
        let now = SystemTime::now();

        for status in self.relay_endpoint_statuses.values_mut() {
            if status.relay_peer_id == Some(relay_peer_id)
                && status.transport_kind == remote_transport
            {
                if status.current_direct_connection_id == Some(connection_id) {
                    status.current_direct_connection_id = None;
                }
                status.last_direct_connection_closed_at = Some(now);
                status.last_management_action = Some(RelayManagementAction::DirectConnectionClosed);
            }
        }
    }

    pub fn record_external_address_candidate(
        &mut self,
        address: Multiaddr,
        source: ExternalAddressSource,
    ) {
        self.record_external_address(address, source, false);
    }

    pub fn record_external_address_confirmed(
        &mut self,
        address: Multiaddr,
        source: ExternalAddressSource,
    ) {
        self.record_external_address(address, source, true);
    }

    pub fn expire_external_address(&mut self, address: &Multiaddr) {
        if let Some(record) = self.external_address_candidates.get_mut(address) {
            record.expired_at = Some(SystemTime::now());
        }
    }

    pub fn list_external_address_candidates(&self) -> Vec<ExternalAddressCandidateRecord> {
        let mut candidates: Vec<_> = self.external_address_candidates.values().cloned().collect();
        candidates.sort_by(|left, right| left.address.to_string().cmp(&right.address.to_string()));
        candidates
    }

    pub fn list_relay_endpoint_statuses(&self) -> Vec<RelayEndpointStatusRecord> {
        let mut statuses: Vec<_> = self.relay_endpoint_statuses.values().cloned().collect();
        statuses.sort_by(|left, right| {
            left.relay_addr
                .to_string()
                .cmp(&right.relay_addr.to_string())
        });
        statuses
    }

    fn record_external_address(
        &mut self,
        address: Multiaddr,
        source: ExternalAddressSource,
        confirmed: bool,
    ) {
        let now = SystemTime::now();
        let record = self
            .external_address_candidates
            .entry(address.clone())
            .or_insert_with(|| ExternalAddressCandidateRecord {
                address: address.clone(),
                transport_kind: address_transport_kind(&address),
                first_observed_at: now,
                last_observed_at: now,
                confirmed_at: None,
                expired_at: None,
                observation_count: 0,
                sources: Vec::new(),
            });

        record.transport_kind = address_transport_kind(&address);
        record.last_observed_at = now;
        record.expired_at = None;
        record.observation_count += 1;

        if !record.sources.contains(&source) {
            record.sources.push(source);
        }

        if confirmed {
            record.confirmed_at = Some(now);
        }
    }
}

pub fn address_transport_kind(addr: &Multiaddr) -> AddressTransportKind {
    if addr.iter().any(|protocol| protocol == Protocol::P2pCircuit) {
        return AddressTransportKind::Relayed;
    }

    for protocol in addr.iter() {
        match protocol {
            Protocol::Tcp(_) => return AddressTransportKind::Tcp,
            Protocol::Udp(_) => return AddressTransportKind::Udp,
            _ => {}
        }
    }

    AddressTransportKind::Other
}

fn relay_peer_id(addr: &Multiaddr) -> Option<PeerId> {
    addr.iter().find_map(|protocol| match protocol {
        Protocol::P2p(peer_id) => Some(peer_id),
        _ => None,
    })
}

fn freshness_windows(transport_kind: AddressTransportKind) -> (Duration, Duration) {
    match transport_kind {
        AddressTransportKind::Udp => (Duration::from_secs(90), Duration::from_secs(300)),
        AddressTransportKind::Tcp => (Duration::from_secs(300), Duration::from_secs(900)),
        AddressTransportKind::Relayed => (Duration::from_secs(600), Duration::from_secs(1800)),
        AddressTransportKind::Other => (Duration::from_secs(300), Duration::from_secs(900)),
    }
}
