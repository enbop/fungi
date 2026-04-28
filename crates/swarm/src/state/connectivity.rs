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
pub enum PeerAddressSource {
    Identify,
    Mdns,
    DeviceConfig,
    DirectCache,
    Manual,
    RelayDerived,
    AutoNat,
    Other,
}

impl PeerAddressSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            PeerAddressSource::Identify => "identify",
            PeerAddressSource::Mdns => "mdns",
            PeerAddressSource::DeviceConfig => "device-config",
            PeerAddressSource::DirectCache => "direct-cache",
            PeerAddressSource::Manual => "manual",
            PeerAddressSource::RelayDerived => "relay-derived",
            PeerAddressSource::AutoNat => "autonat",
            PeerAddressSource::Other => "other",
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
    ListenerMissingReconcile,
    DirectConnectionMissingReconcile,
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
            RelayManagementAction::ListenerMissingReconcile => "listener-missing-reconcile",
            RelayManagementAction::DirectConnectionMissingReconcile => {
                "direct-connection-missing-reconcile"
            }
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
    pub last_direct_connection_established_at: Option<SystemTime>,
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

#[derive(Debug, Clone)]
pub struct PeerAddressRecord {
    pub peer_id: PeerId,
    pub address: Multiaddr,
    pub transport_kind: AddressTransportKind,
    pub source: PeerAddressSource,
    pub first_observed_at: SystemTime,
    pub last_observed_at: SystemTime,
    pub expired_at: Option<SystemTime>,
    pub observation_count: u64,
}

impl PeerAddressRecord {
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
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum PeerAddressObservation {
    New,
    Refreshed,
    Ignored,
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
    peer_address_records: HashMap<(PeerId, Multiaddr), PeerAddressRecord>,
    peer_address_revision: u64,
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
                last_direct_connection_established_at: None,
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
            if status.relay_peer_id == Some(relay_peer_id)
                && status.transport_kind == AddressTransportKind::Tcp
            {
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

    pub fn record_relay_connection_established(
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
                status.current_direct_connection_id = Some(connection_id);
                status.last_direct_connection_established_at = Some(now);
            }
        }
    }

    pub fn record_relay_connection_closed(
        &mut self,
        relay_peer_id: PeerId,
        connection_id: ConnectionId,
        remote_addr: &Multiaddr,
    ) -> bool {
        let remote_transport = address_transport_kind(remote_addr);
        let now = SystemTime::now();
        let mut closed_active_connection = false;

        for status in self.relay_endpoint_statuses.values_mut() {
            if status.relay_peer_id == Some(relay_peer_id)
                && status.transport_kind == remote_transport
            {
                if status.current_direct_connection_id == Some(connection_id) {
                    status.current_direct_connection_id = None;
                    closed_active_connection = true;
                    status.last_direct_connection_closed_at = Some(now);
                    status.last_management_action =
                        Some(RelayManagementAction::DirectConnectionClosed);
                }
            }
        }

        closed_active_connection
    }

    pub fn relay_endpoint_active(&self, relay_addr: &Multiaddr) -> bool {
        self.relay_endpoint_statuses
            .get(relay_addr)
            .and_then(|status| status.current_direct_connection_id)
            .is_some()
    }

    pub fn relay_tcp_ready(&self, relay_peer_id: PeerId) -> bool {
        // UDP/QUIC relay endpoints are observer-only. A UDP refresh is allowed
        // only when the same relay peer already has an active TCP reservation
        // carrier and a registered relay listener.
        self.relay_endpoint_statuses.values().any(|status| {
            status.relay_peer_id == Some(relay_peer_id)
                && status.transport_kind == AddressTransportKind::Tcp
                && status.listener_registered
                && status.current_direct_connection_id.is_some()
        })
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

    pub fn record_peer_address(
        &mut self,
        peer_id: PeerId,
        address: Multiaddr,
        source: PeerAddressSource,
    ) -> PeerAddressObservation {
        let Some(normalized) = normalize_peer_address(&address, peer_id) else {
            return PeerAddressObservation::Ignored;
        };

        let now = SystemTime::now();
        let key = (peer_id, normalized.clone());
        let transport_kind = address_transport_kind(&normalized);

        match self.peer_address_records.get_mut(&key) {
            Some(record) => {
                record.last_observed_at = now;
                record.expired_at = None;
                record.observation_count += 1;
                record.source = source;
                PeerAddressObservation::Refreshed
            }
            None => {
                self.peer_address_records.insert(
                    key,
                    PeerAddressRecord {
                        peer_id,
                        address: normalized,
                        transport_kind,
                        source,
                        first_observed_at: now,
                        last_observed_at: now,
                        expired_at: None,
                        observation_count: 1,
                    },
                );
                self.peer_address_revision += 1;
                PeerAddressObservation::New
            }
        }
    }

    pub fn restore_peer_address_record(
        &mut self,
        peer_id: PeerId,
        address: Multiaddr,
        source: PeerAddressSource,
        first_observed_at: SystemTime,
        last_observed_at: SystemTime,
        observation_count: u64,
    ) -> PeerAddressObservation {
        let Some(normalized) = normalize_peer_address(&address, peer_id) else {
            return PeerAddressObservation::Ignored;
        };

        let first_observed_at = first_observed_at.min(last_observed_at);
        let observation_count = observation_count.max(1);
        let key = (peer_id, normalized.clone());
        let transport_kind = address_transport_kind(&normalized);

        match self.peer_address_records.get_mut(&key) {
            Some(record) => {
                if last_observed_at <= record.last_observed_at {
                    return PeerAddressObservation::Ignored;
                }

                record.first_observed_at = record.first_observed_at.min(first_observed_at);
                record.last_observed_at = last_observed_at;
                record.expired_at = None;
                record.observation_count = record.observation_count.max(observation_count);
                record.source = source;
                PeerAddressObservation::Refreshed
            }
            None => {
                self.peer_address_records.insert(
                    key,
                    PeerAddressRecord {
                        peer_id,
                        address: normalized,
                        transport_kind,
                        source,
                        first_observed_at,
                        last_observed_at,
                        expired_at: None,
                        observation_count,
                    },
                );
                self.peer_address_revision += 1;
                PeerAddressObservation::New
            }
        }
    }

    pub fn expire_peer_address(&mut self, peer_id: PeerId, address: Multiaddr) -> bool {
        let Some(normalized) = normalize_peer_address(&address, peer_id) else {
            return false;
        };

        let key = (peer_id, normalized);
        let Some(record) = self.peer_address_records.get_mut(&key) else {
            return false;
        };

        if record.expired_at.is_none() {
            record.expired_at = Some(SystemTime::now());
        }

        true
    }

    pub fn list_peer_addresses(&self) -> Vec<PeerAddressRecord> {
        let mut records: Vec<_> = self.peer_address_records.values().cloned().collect();
        records.sort_by(|left, right| {
            left.peer_id
                .to_string()
                .cmp(&right.peer_id.to_string())
                .then(left.address.to_string().cmp(&right.address.to_string()))
        });
        records
    }

    pub fn peer_address_revision(&self) -> u64 {
        self.peer_address_revision
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

fn normalize_peer_address(address: &Multiaddr, peer_id: PeerId) -> Option<Multiaddr> {
    let mut normalized = Multiaddr::empty();
    let mut protocols = address.iter().peekable();
    let mut saw_transport = false;

    while let Some(protocol) = protocols.next() {
        match protocol {
            Protocol::Ip4(ip) => {
                if ip.is_unspecified() || ip.is_loopback() {
                    return None;
                }
                normalized.push(Protocol::Ip4(ip));
            }
            Protocol::Ip6(ip) => {
                if ip.is_unspecified() || ip.is_loopback() {
                    return None;
                }
                normalized.push(Protocol::Ip6(ip));
            }
            Protocol::Dns(name) => {
                if is_local_hostname(&name) {
                    return None;
                }
                normalized.push(Protocol::Dns(name));
            }
            Protocol::Dns4(name) => {
                if is_local_hostname(&name) {
                    return None;
                }
                normalized.push(Protocol::Dns4(name));
            }
            Protocol::Dns6(name) => {
                if is_local_hostname(&name) {
                    return None;
                }
                normalized.push(Protocol::Dns6(name));
            }
            Protocol::Dnsaddr(name) => {
                if is_local_hostname(&name) {
                    return None;
                }
                normalized.push(Protocol::Dnsaddr(name));
            }
            Protocol::Tcp(port) => {
                if port == 0 {
                    return None;
                }
                saw_transport = true;
                normalized.push(Protocol::Tcp(port));
            }
            Protocol::Udp(port) => {
                if port == 0 {
                    return None;
                }
                saw_transport = true;
                normalized.push(Protocol::Udp(port));
            }
            Protocol::QuicV1 => normalized.push(Protocol::QuicV1),
            Protocol::P2p(observed_peer_id) => {
                if observed_peer_id != peer_id {
                    return None;
                }

                if protocols.peek().is_none() {
                    break;
                }

                return None;
            }
            Protocol::P2pCircuit => normalized.push(Protocol::P2pCircuit),
            _ => return None,
        }
    }

    if !saw_transport {
        return None;
    }

    Some(normalized)
}

fn is_local_hostname(name: &str) -> bool {
    name.eq_ignore_ascii_case("localhost")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peer_address_normalization_accepts_direct_identify_addr() {
        let peer_id = PeerId::random();
        let address: Multiaddr = format!("/ip4/192.168.1.7/tcp/4001/p2p/{peer_id}")
            .parse()
            .unwrap();

        let normalized = normalize_peer_address(&address, peer_id).unwrap();
        assert_eq!(normalized.to_string(), "/ip4/192.168.1.7/tcp/4001");
    }

    #[test]
    fn peer_address_normalization_rejects_mismatched_peer_suffix() {
        let peer_id = PeerId::random();
        let wrong_peer_id = PeerId::random();
        let address: Multiaddr = format!("/ip4/192.168.1.7/tcp/4001/p2p/{wrong_peer_id}")
            .parse()
            .unwrap();

        assert!(normalize_peer_address(&address, peer_id).is_none());
    }

    #[test]
    fn peer_address_normalization_rejects_unspecified_ip() {
        let peer_id = PeerId::random();
        let address: Multiaddr = format!("/ip4/0.0.0.0/tcp/4001/p2p/{peer_id}")
            .parse()
            .unwrap();

        assert!(normalize_peer_address(&address, peer_id).is_none());
    }

    #[test]
    fn peer_address_normalization_rejects_loopback_ip() {
        let peer_id = PeerId::random();
        let address: Multiaddr = format!("/ip4/127.0.0.1/tcp/4001/p2p/{peer_id}")
            .parse()
            .unwrap();

        assert!(normalize_peer_address(&address, peer_id).is_none());
    }

    #[test]
    fn peer_address_normalization_rejects_localhost_dns() {
        let peer_id = PeerId::random();
        let address: Multiaddr = format!("/dns4/localhost/tcp/4001/p2p/{peer_id}")
            .parse()
            .unwrap();

        assert!(normalize_peer_address(&address, peer_id).is_none());
    }

    #[test]
    fn peer_address_normalization_rejects_zero_port() {
        let peer_id = PeerId::random();
        let address: Multiaddr = format!("/ip4/192.168.1.7/tcp/0/p2p/{peer_id}")
            .parse()
            .unwrap();

        assert!(normalize_peer_address(&address, peer_id).is_none());
    }

    #[test]
    fn peer_address_record_deduplicates_and_counts_observations() {
        let peer_id = PeerId::random();
        let address: Multiaddr = format!("/ip4/192.168.1.7/tcp/4001/p2p/{peer_id}")
            .parse()
            .unwrap();
        let mut state = ConnectivityState::default();

        assert_eq!(
            state.record_peer_address(peer_id, address.clone(), PeerAddressSource::Identify),
            PeerAddressObservation::New
        );
        assert_eq!(
            state.record_peer_address(peer_id, address, PeerAddressSource::Identify),
            PeerAddressObservation::Refreshed
        );

        let records = state.list_peer_addresses();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].observation_count, 2);
        assert!(records[0].expired_at.is_none());
    }

    #[test]
    fn peer_address_revision_only_changes_for_new_addresses() {
        let peer_id = PeerId::random();
        let address: Multiaddr = format!("/ip4/192.168.1.7/tcp/4001/p2p/{peer_id}")
            .parse()
            .unwrap();
        let mut state = ConnectivityState::default();

        assert_eq!(state.peer_address_revision(), 0);
        assert_eq!(
            state.record_peer_address(peer_id, address.clone(), PeerAddressSource::Identify),
            PeerAddressObservation::New
        );
        assert_eq!(state.peer_address_revision(), 1);
        assert_eq!(
            state.record_peer_address(peer_id, address, PeerAddressSource::Identify),
            PeerAddressObservation::Refreshed
        );
        assert_eq!(state.peer_address_revision(), 1);
    }

    #[test]
    fn peer_address_expire_marks_record_expired_until_refreshed() {
        let peer_id = PeerId::random();
        let address: Multiaddr = format!("/ip4/192.168.1.7/tcp/4001/p2p/{peer_id}")
            .parse()
            .unwrap();
        let mut state = ConnectivityState::default();

        assert_eq!(
            state.record_peer_address(peer_id, address.clone(), PeerAddressSource::Mdns),
            PeerAddressObservation::New
        );
        assert!(state.expire_peer_address(peer_id, address.clone()));
        assert_eq!(
            state.list_peer_addresses()[0].freshness(SystemTime::now()),
            AddressFreshness::Expired
        );

        assert_eq!(
            state.record_peer_address(peer_id, address, PeerAddressSource::Mdns),
            PeerAddressObservation::Refreshed
        );
        let records = state.list_peer_addresses();
        assert!(records[0].expired_at.is_none());
        assert_ne!(
            records[0].freshness(SystemTime::now()),
            AddressFreshness::Expired
        );
    }

    #[test]
    fn peer_address_restore_preserves_cached_freshness() {
        let peer_id = PeerId::random();
        let address: Multiaddr = format!("/ip4/192.168.1.7/tcp/4001/p2p/{peer_id}")
            .parse()
            .unwrap();
        let mut state = ConnectivityState::default();
        let now = SystemTime::now();
        let old_last_observed_at = now - Duration::from_secs(3600);
        let old_first_observed_at = old_last_observed_at - Duration::from_secs(60);

        assert_eq!(
            state.restore_peer_address_record(
                peer_id,
                address,
                PeerAddressSource::DirectCache,
                old_first_observed_at,
                old_last_observed_at,
                7,
            ),
            PeerAddressObservation::New
        );

        let records = state.list_peer_addresses();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].first_observed_at, old_first_observed_at);
        assert_eq!(records[0].last_observed_at, old_last_observed_at);
        assert_eq!(records[0].observation_count, 7);
        assert_eq!(records[0].freshness(now), AddressFreshness::Stale);
    }

    #[test]
    fn relay_connection_close_only_clears_current_direct_connection() {
        let relay_addr: Multiaddr = "/ip4/160.16.206.21/udp/30001/quic-v1/p2p/12D3KooWQd4YvW5yV7k3K14rH4VJm6AqW4U4GQnVx2q4iH7z3fAr"
            .parse()
            .unwrap();
        let relay_peer_id = relay_peer_id(&relay_addr).unwrap();
        let mut state = ConnectivityState::default();
        state.register_relay_endpoint(relay_addr.clone());

        let old_connection = ConnectionId::new_unchecked(7);
        let current_connection = ConnectionId::new_unchecked(8);

        state.record_relay_connection_established(relay_peer_id, old_connection, &relay_addr);
        state.record_relay_connection_established(relay_peer_id, current_connection, &relay_addr);

        assert!(!state.record_relay_connection_closed(relay_peer_id, old_connection, &relay_addr));
        assert!(state.relay_endpoint_active(&relay_addr));

        assert!(state.record_relay_connection_closed(
            relay_peer_id,
            current_connection,
            &relay_addr
        ));
        assert!(!state.relay_endpoint_active(&relay_addr));
    }

    #[test]
    fn relay_peer_tcp_health_requires_tcp_listener_and_direct_connection() {
        let relay_peer_id = PeerId::random();
        let tcp_addr: Multiaddr = format!("/ip4/160.16.206.21/tcp/30001/p2p/{relay_peer_id}")
            .parse()
            .unwrap();
        let udp_addr: Multiaddr =
            format!("/ip4/160.16.206.21/udp/30001/quic-v1/p2p/{relay_peer_id}")
                .parse()
                .unwrap();
        let mut state = ConnectivityState::default();
        state.register_relay_endpoint(tcp_addr.clone());
        state.register_relay_endpoint(udp_addr.clone());

        state.record_relay_connection_established(
            relay_peer_id,
            ConnectionId::new_unchecked(7),
            &udp_addr,
        );
        state.record_relay_listener_check(&udp_addr, true);
        assert!(!state.relay_tcp_ready(relay_peer_id));

        state.record_relay_connection_established(
            relay_peer_id,
            ConnectionId::new_unchecked(8),
            &tcp_addr,
        );
        assert!(!state.relay_tcp_ready(relay_peer_id));

        state.record_relay_listener_check(&tcp_addr, true);
        assert!(state.relay_tcp_ready(relay_peer_id));

        assert!(state.record_relay_connection_closed(
            relay_peer_id,
            ConnectionId::new_unchecked(8),
            &tcp_addr
        ));
        assert!(!state.relay_tcp_ready(relay_peer_id));
    }

    #[test]
    fn relay_reservation_accept_updates_only_tcp_endpoint() {
        let relay_peer_id = PeerId::random();
        let tcp_addr: Multiaddr = format!("/ip4/160.16.206.21/tcp/30001/p2p/{relay_peer_id}")
            .parse()
            .unwrap();
        let udp_addr: Multiaddr =
            format!("/ip4/160.16.206.21/udp/30001/quic-v1/p2p/{relay_peer_id}")
                .parse()
                .unwrap();
        let mut state = ConnectivityState::default();
        state.register_relay_endpoint(tcp_addr.clone());
        state.register_relay_endpoint(udp_addr.clone());

        let tcp_connection_id = ConnectionId::new_unchecked(8);
        let udp_connection_id = ConnectionId::new_unchecked(9);
        let direct_connections = vec![
            RelayDirectConnectionSnapshot {
                transport_kind: AddressTransportKind::Tcp,
                connection_id: tcp_connection_id,
            },
            RelayDirectConnectionSnapshot {
                transport_kind: AddressTransportKind::Udp,
                connection_id: udp_connection_id,
            },
        ];

        state.record_relay_reservation_accepted(
            relay_peer_id,
            RelayManagementAction::ReservationEstablished,
            &direct_connections,
        );

        let statuses = state.list_relay_endpoint_statuses();
        let tcp_status = statuses
            .iter()
            .find(|status| status.relay_addr == tcp_addr)
            .unwrap();
        let udp_status = statuses
            .iter()
            .find(|status| status.relay_addr == udp_addr)
            .unwrap();

        assert_eq!(
            tcp_status.last_management_action,
            Some(RelayManagementAction::ReservationEstablished)
        );
        assert_eq!(
            tcp_status.current_direct_connection_id,
            Some(tcp_connection_id)
        );
        assert!(tcp_status.last_reservation_established_at.is_some());
        assert_eq!(udp_status.last_management_action, None);
        assert_eq!(udp_status.current_direct_connection_id, None);
        assert!(udp_status.last_reservation_established_at.is_none());
    }
}
