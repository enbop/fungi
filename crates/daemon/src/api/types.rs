use std::time::SystemTime;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct ConnectionSnapshot {
    pub peer_id: String,
    pub connection_id: String,
    pub direction: String,
    pub remote_addr: String,
    pub is_relay: bool,
    pub last_rtt_ms: u64,
    pub last_ping_at: Option<SystemTime>,
    pub active_streams_total: usize,
    pub active_streams_by_protocol: Vec<ProtocolStreamCountSnapshot>,
}

#[derive(Debug, Clone)]
pub struct ProtocolStreamCountSnapshot {
    pub protocol_name: String,
    pub stream_count: usize,
}

#[derive(Debug, Clone)]
pub struct ActiveStreamSnapshot {
    pub stream_id: u64,
    pub peer_id: String,
    pub connection_id: String,
    pub protocol_name: String,
    pub opened_at: SystemTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceAccess {
    pub peer_id: String,
    pub service_id: String,
    pub service_name: String,
    pub endpoints: Vec<ServiceAccessEndpoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceAccessEndpoint {
    pub name: String,
    pub protocol: String,
    pub local_host: String,
    pub local_port: u16,
}
