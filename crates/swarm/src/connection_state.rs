use crate::{SwarmControl, peer_handshake::PeerHandshakePayload};
use async_result::Completer;
use libp2p::{
    Multiaddr, PeerId,
    core::ConnectedPoint,
    swarm::{ConnectionId, DialError},
};
use parking_lot::{Mutex, RwLock};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

pub type DialCallback = Arc<Mutex<HashMap<PeerId, Completer<std::result::Result<(), DialError>>>>>;

#[derive(Debug, Clone)]
pub struct ConnectionInfo {
    connection_id: ConnectionId,
    multiaddr: Multiaddr,
}

impl ConnectionInfo {
    pub fn new(connection_id: ConnectionId, multiaddr: Multiaddr) -> Self {
        Self {
            connection_id,
            multiaddr,
        }
    }

    pub fn connection_id(&self) -> ConnectionId {
        self.connection_id
    }

    pub fn multiaddr(&self) -> &Multiaddr {
        &self.multiaddr
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ConnectionDirection {
    Inbound,
    Outbound,
}

#[derive(Debug, Default, Clone)]
pub struct PeerConnections {
    handshake: Option<PeerHandshakePayload>,
    inbound: Vec<ConnectionInfo>,
    outbound: Vec<ConnectionInfo>,
}

impl PeerConnections {
    pub fn update_handshake(&mut self, handshake: PeerHandshakePayload) {
        self.handshake = Some(handshake);
    }

    pub fn host_name(&self) -> Option<String> {
        self.handshake.as_ref().and_then(|h| h.host_name())
    }

    pub fn inbound(&self) -> &[ConnectionInfo] {
        &self.inbound
    }

    pub fn outbound(&self) -> &[ConnectionInfo] {
        &self.outbound
    }

    pub fn total_connections(&self) -> usize {
        self.inbound.len() + self.outbound.len()
    }

    pub(crate) fn add_connection(&mut self, direction: ConnectionDirection, info: ConnectionInfo) {
        match direction {
            ConnectionDirection::Inbound => self.inbound.push(info),
            ConnectionDirection::Outbound => self.outbound.push(info),
        }
    }

    pub(crate) fn remove_connection(&mut self, connection_id: ConnectionId) -> bool {
        let before = self.total_connections();
        self.inbound
            .retain(|info| info.connection_id() != connection_id);
        self.outbound
            .retain(|info| info.connection_id() != connection_id);
        self.total_connections() != before
    }
}

#[derive(Default, Clone)]
pub struct State {
    dial_callback: DialCallback,
    peer_connections: Arc<Mutex<HashMap<PeerId, PeerConnections>>>,
    connection_id_map: Arc<Mutex<HashMap<ConnectionId, PeerId>>>,
    incoming_allowed_peers: Arc<RwLock<HashSet<PeerId>>>,
}

impl State {
    pub fn new(incoming_allowed_peers: HashSet<PeerId>) -> Self {
        Self {
            dial_callback: Arc::new(Mutex::new(HashMap::new())),
            peer_connections: Arc::new(Mutex::new(HashMap::new())),
            connection_id_map: Arc::new(Mutex::new(HashMap::new())),
            incoming_allowed_peers: Arc::new(RwLock::new(incoming_allowed_peers)),
        }
    }

    pub fn dial_callback(&self) -> DialCallback {
        self.dial_callback.clone()
    }

    pub fn peer_connections(&self) -> Arc<Mutex<HashMap<PeerId, PeerConnections>>> {
        self.peer_connections.clone()
    }

    pub fn connection_id_map(&self) -> Arc<Mutex<HashMap<ConnectionId, PeerId>>> {
        self.connection_id_map.clone()
    }

    pub fn incoming_allowed_peers(&self) -> Arc<RwLock<HashSet<PeerId>>> {
        self.incoming_allowed_peers.clone()
    }

    pub fn get_incoming_allowed_peers_list(&self) -> Vec<PeerId> {
        self.incoming_allowed_peers.read().iter().cloned().collect()
    }

    pub fn peer_id_by_connection_id(&self, connection_id: &ConnectionId) -> Option<PeerId> {
        self.connection_id_map.lock().get(connection_id).cloned()
    }

    pub fn get_peer_connections(&self, peer_id: &PeerId) -> Option<PeerConnections> {
        self.peer_connections.lock().get(peer_id).cloned()
    }

    pub fn has_active_connection(&self, peer_id: &PeerId) -> bool {
        self.peer_connections
            .lock()
            .get(peer_id)
            .map_or(false, |peers| peers.total_connections() > 0)
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
    let connection_info = ConnectionInfo::new(connection_id, endpoint.get_remote_address().clone());

    let state = swarm_control.state();
    state
        .connection_id_map()
        .lock()
        .insert(connection_id, peer_id);
    state
        .peer_connections()
        .lock()
        .entry(peer_id)
        .or_default()
        .add_connection(direction, connection_info);
}

pub(crate) fn handle_connection_closed(
    swarm_control: &SwarmControl,
    peer_id: PeerId,
    connection_id: ConnectionId,
) {
    let state = swarm_control.state();

    state.connection_id_map().lock().remove(&connection_id);

    let peers = state.peer_connections();
    let mut peers = peers.lock();
    if let Some(connections) = peers.get_mut(&peer_id) {
        connections.remove_connection(connection_id);
        if connections.total_connections() == 0 {
            peers.remove(&peer_id);
        }
    }
}
