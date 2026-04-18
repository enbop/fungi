use crate::ConnectionDirection;
use libp2p::{Multiaddr, swarm::ConnectionId};
use std::time::{Duration, SystemTime};

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
