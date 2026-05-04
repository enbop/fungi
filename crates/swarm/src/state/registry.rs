use crate::{
    AddressTransportKind, ConnectivityState, ExternalAddressCandidateRecord, ExternalAddressSource,
    PeerAddressRecord, PeerAddressSource, RelayDirectConnectionSnapshot, RelayEndpointStatusRecord,
    SwarmControl,
};
use async_result::Completer;
use libp2p::{
    Multiaddr, PeerId, StreamProtocol,
    core::ConnectedPoint,
    multiaddr::Protocol,
    swarm::{ConnectionId, DialError},
};
use parking_lot::{Mutex, RwLock};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime},
};

pub type DialCallback = Arc<Mutex<HashMap<PeerId, Completer<std::result::Result<(), DialError>>>>>;

#[derive(Debug, Clone, Copy)]
pub enum ConnectionDirection {
    Inbound,
    Outbound,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ConnectionGovernanceState {
    Unknown,
    Recommended,
    Deprecated,
    Closing,
}

impl ConnectionGovernanceState {
    pub fn as_str(&self) -> &'static str {
        match self {
            ConnectionGovernanceState::Unknown => "unknown",
            ConnectionGovernanceState::Recommended => "recommended",
            ConnectionGovernanceState::Deprecated => "deprecated",
            ConnectionGovernanceState::Closing => "closing",
        }
    }
}

impl Default for ConnectionGovernanceState {
    fn default() -> Self {
        Self::Unknown
    }
}

#[derive(Debug, Clone, Default)]
pub struct ConnectionGovernanceInfo {
    pub state: ConnectionGovernanceState,
    pub reason: Option<String>,
    pub changed_at: Option<SystemTime>,
}

#[derive(Debug, Clone, Default)]
pub struct ConnectionPingInfo {
    pub last_rtt: Option<Duration>,
    pub last_rtt_at: Option<SystemTime>,
}

#[derive(Debug, Clone)]
pub struct ConnectionRecord {
    pub peer_id: PeerId,
    pub connection_id: ConnectionId,
    pub direction: ConnectionDirection,
    pub remote_addr: Multiaddr,
    pub established_at: SystemTime,
    pub ping_info: ConnectionPingInfo,
    pub governance: ConnectionGovernanceInfo,
}

impl ConnectionRecord {
    pub fn connection_id(&self) -> ConnectionId {
        self.connection_id
    }

    pub fn multiaddr(&self) -> &Multiaddr {
        &self.remote_addr
    }

    pub fn is_relay(&self) -> bool {
        self.remote_addr
            .iter()
            .any(|protocol| matches!(protocol, Protocol::P2pCircuit))
    }
}

#[derive(Debug, Default)]
pub struct ConnectionIndexes {
    pub by_id: HashMap<ConnectionId, ConnectionRecord>,
    pub by_peer: HashMap<PeerId, Vec<ConnectionId>>,
}

impl ConnectionIndexes {
    pub fn connection_len(&self) -> usize {
        self.by_id.len()
    }

    pub fn peer_len(&self) -> usize {
        self.by_peer.len()
    }

    pub fn connection_len_for_peer(&self, peer_id: &PeerId) -> usize {
        self.by_peer.get(peer_id).map_or(0, Vec::len)
    }

    fn insert(&mut self, record: ConnectionRecord) {
        let peer_id = record.peer_id;
        let connection_id = record.connection_id;
        self.by_id.insert(connection_id, record);
        self.by_peer.entry(peer_id).or_default().push(connection_id);
    }

    fn remove(&mut self, connection_id: ConnectionId) -> Option<ConnectionRecord> {
        let record = self.by_id.remove(&connection_id)?;

        if let Some(connection_ids) = self.by_peer.get_mut(&record.peer_id) {
            connection_ids.retain(|id| *id != connection_id);
            if connection_ids.is_empty() {
                self.by_peer.remove(&record.peer_id);
            }
        }

        Some(record)
    }

    fn records_for_peer(&self, peer_id: &PeerId) -> Vec<ConnectionRecord> {
        self.by_peer
            .get(peer_id)
            .into_iter()
            .flat_map(|connection_ids| connection_ids.iter())
            .filter_map(|connection_id| self.by_id.get(connection_id).cloned())
            .collect()
    }
}

/// Runtime registry for peer connections, ping info, stream observations and connectivity
/// diagnostics.
#[derive(Default, Clone)]
pub struct State {
    dial_callback: DialCallback,
    connections: Arc<Mutex<ConnectionIndexes>>,
    incoming_allowed_peers: Arc<RwLock<HashSet<PeerId>>>,
    next_stream_id: Arc<AtomicU64>,
    stream_state: Arc<Mutex<StreamObservationState>>,
    connectivity_state: Arc<Mutex<ConnectivityState>>,
}

impl State {
    pub fn new(incoming_allowed_peers: HashSet<PeerId>) -> Self {
        Self {
            dial_callback: Arc::new(Mutex::new(HashMap::new())),
            connections: Arc::new(Mutex::new(ConnectionIndexes::default())),
            incoming_allowed_peers: Arc::new(RwLock::new(incoming_allowed_peers)),
            next_stream_id: Arc::new(AtomicU64::new(0)),
            stream_state: Arc::new(Mutex::new(StreamObservationState::default())),
            connectivity_state: Arc::new(Mutex::new(ConnectivityState::default())),
        }
    }

    pub fn dial_callback(&self) -> DialCallback {
        self.dial_callback.clone()
    }

    pub fn incoming_allowed_peers(&self) -> Arc<RwLock<HashSet<PeerId>>> {
        self.incoming_allowed_peers.clone()
    }

    pub fn register_relay_endpoint(&self, relay_addr: Multiaddr) {
        self.connectivity_state
            .lock()
            .register_relay_endpoint(relay_addr);
    }

    pub fn set_relay_task_running(&self, relay_addr: &Multiaddr, task_running: bool) {
        self.connectivity_state
            .lock()
            .set_relay_task_running(relay_addr, task_running);
    }

    pub fn record_relay_listener_check(&self, relay_addr: &Multiaddr, listener_registered: bool) {
        self.connectivity_state
            .lock()
            .record_relay_listener_check(relay_addr, listener_registered);
    }

    pub fn record_relay_management_action(
        &self,
        relay_addr: &Multiaddr,
        action: crate::RelayManagementAction,
    ) {
        self.connectivity_state
            .lock()
            .record_relay_management_action(relay_addr, action);
    }

    pub fn record_relay_management_error(&self, relay_addr: &Multiaddr, error: impl Into<String>) {
        self.connectivity_state
            .lock()
            .record_relay_management_error(relay_addr, error);
    }

    pub fn record_relay_reservation_accepted(
        &self,
        relay_peer_id: PeerId,
        change: crate::RelayManagementAction,
    ) {
        let direct_connections = self.current_direct_relay_connections(relay_peer_id);
        self.connectivity_state
            .lock()
            .record_relay_reservation_accepted(relay_peer_id, change, &direct_connections);
    }

    pub fn record_relay_connection_closed(
        &self,
        relay_peer_id: PeerId,
        connection_id: ConnectionId,
        remote_addr: &Multiaddr,
    ) -> bool {
        self.connectivity_state
            .lock()
            .record_relay_connection_closed(relay_peer_id, connection_id, remote_addr)
    }

    pub fn record_relay_connection_established(
        &self,
        relay_peer_id: PeerId,
        connection_id: ConnectionId,
        remote_addr: &Multiaddr,
    ) {
        self.connectivity_state
            .lock()
            .record_relay_connection_established(relay_peer_id, connection_id, remote_addr);
    }

    pub fn relay_endpoint_active(&self, relay_addr: &Multiaddr) -> bool {
        self.connectivity_state
            .lock()
            .relay_endpoint_active(relay_addr)
    }

    pub fn relay_tcp_ready(&self, relay_peer_id: PeerId) -> bool {
        self.connectivity_state
            .lock()
            .relay_tcp_ready(relay_peer_id)
    }

    pub fn record_external_address_candidate(
        &self,
        address: Multiaddr,
        source: ExternalAddressSource,
    ) {
        self.connectivity_state
            .lock()
            .record_external_address_candidate(address, source);
    }

    pub fn record_external_address_confirmed(
        &self,
        address: Multiaddr,
        source: ExternalAddressSource,
    ) {
        self.connectivity_state
            .lock()
            .record_external_address_confirmed(address, source);
    }

    pub fn expire_external_address(&self, address: &Multiaddr) {
        self.connectivity_state
            .lock()
            .expire_external_address(address);
    }

    pub fn list_external_address_candidates(&self) -> Vec<ExternalAddressCandidateRecord> {
        self.connectivity_state
            .lock()
            .list_external_address_candidates()
    }

    pub fn list_relay_endpoint_statuses(&self) -> Vec<RelayEndpointStatusRecord> {
        self.connectivity_state
            .lock()
            .list_relay_endpoint_statuses()
    }

    pub fn record_peer_address(
        &self,
        peer_id: PeerId,
        address: Multiaddr,
        source: PeerAddressSource,
    ) -> crate::PeerAddressObservation {
        self.connectivity_state
            .lock()
            .record_peer_address(peer_id, address, source)
    }

    pub fn restore_peer_address_record(
        &self,
        peer_id: PeerId,
        address: Multiaddr,
        source: PeerAddressSource,
        first_observed_at: SystemTime,
        last_observed_at: SystemTime,
        observation_count: u64,
    ) -> crate::PeerAddressObservation {
        self.connectivity_state.lock().restore_peer_address_record(
            peer_id,
            address,
            source,
            first_observed_at,
            last_observed_at,
            observation_count,
        )
    }

    pub fn expire_peer_address(&self, peer_id: PeerId, address: Multiaddr) -> bool {
        self.connectivity_state
            .lock()
            .expire_peer_address(peer_id, address)
    }

    pub fn list_peer_addresses(&self) -> Vec<PeerAddressRecord> {
        self.connectivity_state.lock().list_peer_addresses()
    }

    pub fn peer_address_revision(&self) -> u64 {
        self.connectivity_state.lock().peer_address_revision()
    }

    pub fn get_incoming_allowed_peers_list(&self) -> Vec<PeerId> {
        self.incoming_allowed_peers.read().iter().cloned().collect()
    }

    pub fn peer_id_by_connection_id(&self, connection_id: &ConnectionId) -> Option<PeerId> {
        self.connections
            .lock()
            .by_id
            .get(connection_id)
            .map(|entry| entry.peer_id)
    }

    pub fn connection_ping_info(&self, connection_id: &ConnectionId) -> Option<ConnectionPingInfo> {
        self.connections
            .lock()
            .by_id
            .get(connection_id)
            .map(|entry| entry.ping_info.clone())
    }

    pub fn connection_governance_info(
        &self,
        connection_id: &ConnectionId,
    ) -> Option<ConnectionGovernanceInfo> {
        self.connections
            .lock()
            .by_id
            .get(connection_id)
            .map(|entry| entry.governance.clone())
    }

    pub fn update_connection_governance(
        &self,
        connection_id: &ConnectionId,
        governance_state: ConnectionGovernanceState,
        reason: Option<String>,
    ) {
        if let Some(entry) = self.connections.lock().by_id.get_mut(connection_id) {
            let reason_changed = entry.governance.reason != reason;
            if entry.governance.state != governance_state || reason_changed {
                entry.governance.changed_at = Some(SystemTime::now());
            }
            entry.governance.state = governance_state;
            entry.governance.reason = reason;
        }
    }

    pub fn update_connection_ping(&self, connection_id: &ConnectionId, rtt: Duration) {
        if let Some(entry) = self.connections.lock().by_id.get_mut(connection_id) {
            entry.ping_info.last_rtt = Some(rtt);
            entry.ping_info.last_rtt_at = Some(SystemTime::now());
        }
    }

    pub fn connection_established_at(&self, connection_id: &ConnectionId) -> Option<SystemTime> {
        self.connections
            .lock()
            .by_id
            .get(connection_id)
            .map(|entry| entry.established_at)
    }

    pub fn track_outbound_stream_opened(
        &self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        protocol: StreamProtocol,
    ) -> StreamObservationHandle {
        let stream_id: u64 = self.next_stream_id.fetch_add(1, Ordering::Relaxed) + 1;

        let mut stream_state = self.stream_state.lock();

        stream_state.streams_by_id.insert(
            stream_id,
            ObservedStreamEntry {
                stream_id,
                peer_id,
                connection_id,
                protocol: protocol.clone(),
                opened_at: SystemTime::now(),
            },
        );

        stream_state
            .stream_ids_by_connection
            .entry(connection_id)
            .or_default()
            .insert(stream_id);
        stream_state
            .stream_ids_by_protocol
            .entry(protocol)
            .or_default()
            .insert(stream_id);
        stream_state
            .stream_ids_by_peer
            .entry(peer_id)
            .or_default()
            .insert(stream_id);

        StreamObservationHandle::new(self.clone(), stream_id)
    }

    pub fn active_streams_by_connection(
        &self,
        connection_id: &ConnectionId,
    ) -> Vec<ObservedStreamEntry> {
        let stream_state = self.stream_state.lock();
        stream_state
            .stream_ids_by_connection
            .get(connection_id)
            .into_iter()
            .flat_map(|ids| ids.iter())
            .filter_map(|stream_id| stream_state.streams_by_id.get(stream_id).cloned())
            .collect()
    }

    pub fn active_streams_by_protocol(
        &self,
        protocol: &StreamProtocol,
    ) -> Vec<ObservedStreamEntry> {
        let stream_state = self.stream_state.lock();
        stream_state
            .stream_ids_by_protocol
            .get(protocol)
            .into_iter()
            .flat_map(|ids| ids.iter())
            .filter_map(|stream_id| stream_state.streams_by_id.get(stream_id).cloned())
            .collect()
    }

    pub fn connection_active_stream_protocol_counts(
        &self,
        connection_id: &ConnectionId,
    ) -> Vec<(String, usize)> {
        let streams = self.active_streams_by_connection(connection_id);
        let mut counts = HashMap::new();
        for stream in streams {
            *counts.entry(stream.protocol.to_string()).or_insert(0) += 1;
        }
        let mut out: Vec<(_, usize)> = counts.into_iter().collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    pub fn list_active_streams(&self) -> Vec<ObservedStreamEntry> {
        let stream_state = self.stream_state.lock();
        let mut streams: Vec<ObservedStreamEntry> =
            stream_state.streams_by_id.values().cloned().collect();
        streams.sort_by(|a, b| a.stream_id.cmp(&b.stream_id));
        streams
    }

    fn mark_stream_closed(&self, stream_id: StreamId) {
        let mut stream_state = self.stream_state.lock();
        let Some(entry) = stream_state.streams_by_id.remove(&stream_id) else {
            return;
        };

        if let Some(ids) = stream_state
            .stream_ids_by_connection
            .get_mut(&entry.connection_id)
        {
            ids.remove(&stream_id);
            if ids.is_empty() {
                stream_state
                    .stream_ids_by_connection
                    .remove(&entry.connection_id);
            }
        }

        if let Some(ids) = stream_state.stream_ids_by_protocol.get_mut(&entry.protocol) {
            ids.remove(&stream_id);
            if ids.is_empty() {
                stream_state.stream_ids_by_protocol.remove(&entry.protocol);
            }
        }

        if let Some(ids) = stream_state.stream_ids_by_peer.get_mut(&entry.peer_id) {
            ids.remove(&stream_id);
            if ids.is_empty() {
                stream_state.stream_ids_by_peer.remove(&entry.peer_id);
            }
        }
    }

    fn close_all_streams_for_connection(&self, connection_id: ConnectionId) {
        let stream_ids: Vec<StreamId> = {
            let stream_state = self.stream_state.lock();
            stream_state
                .stream_ids_by_connection
                .get(&connection_id)
                .into_iter()
                .flat_map(|ids| ids.iter().copied())
                .collect()
        };

        for stream_id in stream_ids {
            self.mark_stream_closed(stream_id);
        }
    }

    pub fn get_connections_by_peer_id(&self, peer_id: &PeerId) -> Vec<ConnectionRecord> {
        self.connections.lock().records_for_peer(peer_id)
    }

    pub fn connected_peer_ids(&self) -> Vec<PeerId> {
        self.connections.lock().by_peer.keys().copied().collect()
    }

    pub fn connection_len_for_peer(&self, peer_id: &PeerId) -> usize {
        self.connections.lock().connection_len_for_peer(peer_id)
    }

    fn current_direct_relay_connections(
        &self,
        peer_id: PeerId,
    ) -> Vec<RelayDirectConnectionSnapshot> {
        let mut direct_connections = Vec::new();

        for connection in self.get_connections_by_peer_id(&peer_id) {
            if !matches!(connection.direction, ConnectionDirection::Outbound) {
                continue;
            }

            let transport_kind = crate::address_transport_kind(connection.multiaddr());
            if matches!(
                transport_kind,
                AddressTransportKind::Tcp | AddressTransportKind::Udp
            ) && !direct_connections
                .iter()
                .any(|snapshot: &RelayDirectConnectionSnapshot| {
                    snapshot.transport_kind == transport_kind
                })
            {
                direct_connections.push(RelayDirectConnectionSnapshot {
                    transport_kind,
                    connection_id: connection.connection_id,
                });
            }
        }

        direct_connections
    }
}

pub type StreamId = u64;

#[derive(Debug, Clone)]
pub struct ObservedStreamEntry {
    pub stream_id: StreamId,
    pub peer_id: PeerId,
    pub connection_id: ConnectionId,
    pub protocol: StreamProtocol,
    pub opened_at: SystemTime,
}

// TODO make it simple
#[derive(Debug, Default)]
struct StreamObservationState {
    streams_by_id: HashMap<StreamId, ObservedStreamEntry>,
    stream_ids_by_connection: HashMap<ConnectionId, HashSet<StreamId>>,
    stream_ids_by_protocol: HashMap<StreamProtocol, HashSet<StreamId>>,
    stream_ids_by_peer: HashMap<PeerId, HashSet<StreamId>>,
}

#[derive(Clone)]
pub struct StreamObservationHandle {
    inner: Arc<StreamObservationHandleInner>,
}

impl StreamObservationHandle {
    fn new(state: State, stream_id: StreamId) -> Self {
        Self {
            inner: Arc::new(StreamObservationHandleInner { state, stream_id }),
        }
    }

    pub fn stream_id(&self) -> StreamId {
        self.inner.stream_id
    }
}

struct StreamObservationHandleInner {
    state: State,
    stream_id: StreamId,
}

impl Drop for StreamObservationHandleInner {
    fn drop(&mut self) {
        self.state.mark_stream_closed(self.stream_id);
    }
}

pub(crate) fn handle_connection_established(
    swarm_control: &SwarmControl,
    peer_id: PeerId,
    connection_id: ConnectionId,
    endpoint: &ConnectedPoint,
) {
    if let Some(completer) = swarm_control
        .state()
        .dial_callback()
        .lock()
        .remove(&peer_id)
    {
        completer.complete(Ok(()));
    }

    let direction = match endpoint {
        ConnectedPoint::Dialer { .. } => ConnectionDirection::Outbound,
        ConnectedPoint::Listener { .. } => ConnectionDirection::Inbound,
    };
    let record = ConnectionRecord {
        peer_id,
        connection_id,
        direction,
        remote_addr: endpoint.get_remote_address().clone(),
        established_at: SystemTime::now(),
        ping_info: ConnectionPingInfo::default(),
        governance: ConnectionGovernanceInfo::default(),
    };

    swarm_control.state().connections.lock().insert(record);
}

pub(crate) fn handle_connection_closed(
    swarm_control: &SwarmControl,
    peer_id: PeerId,
    connection_id: ConnectionId,
) {
    let state = swarm_control.state();

    state.close_all_streams_for_connection(connection_id);
    let _ = peer_id;
    state.connections.lock().remove(connection_id);
}
