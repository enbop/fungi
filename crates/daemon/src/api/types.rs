use std::time::SystemTime;

use fungi_swarm::{ExternalAddressCandidateRecord, PeerAddressRecord, RelayEndpointStatusRecord};
use serde::{Deserialize, Serialize};

/// Daemon-layer DTO for external address observability.
///
/// These snapshot structs exist to decouple the daemon/API surface from internal swarm state and
/// to provide stable, serialization-friendly data for CLI, gRPC and future UI layers.
#[derive(Debug, Clone)]
pub struct ExternalAddressSnapshot {
    pub address: String,
    pub transport: String,
    pub freshness: String,
    pub recommend_refresh_before_dcutr: bool,
    pub first_observed_at: SystemTime,
    pub last_observed_at: SystemTime,
    pub confirmed_at: Option<SystemTime>,
    pub expired_at: Option<SystemTime>,
    pub observation_count: u64,
    pub sources: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct RelayEndpointStatusSnapshot {
    pub relay_addr: String,
    pub relay_peer_id: Option<String>,
    pub transport: String,
    pub listener_registered: bool,
    pub task_running: bool,
    pub current_direct_connection_id: Option<String>,
    pub last_listener_seen_at: Option<SystemTime>,
    pub last_listener_missing_at: Option<SystemTime>,
    pub last_reservation_accepted_at: Option<SystemTime>,
    pub last_reservation_established_at: Option<SystemTime>,
    pub last_reservation_renewed_at: Option<SystemTime>,
    pub last_direct_connection_closed_at: Option<SystemTime>,
    pub last_management_action: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PeerAddressSnapshot {
    pub peer_id: String,
    pub address: String,
    pub transport: String,
    pub source: String,
    pub freshness: String,
    pub first_observed_at: SystemTime,
    pub last_observed_at: SystemTime,
    pub expired_at: Option<SystemTime>,
    pub observation_count: u64,
}

impl From<ExternalAddressCandidateRecord> for ExternalAddressSnapshot {
    fn from(record: ExternalAddressCandidateRecord) -> Self {
        let now = SystemTime::now();
        Self {
            address: record.address.to_string(),
            transport: record.transport_kind.as_str().to_string(),
            freshness: record.freshness(now).as_str().to_string(),
            recommend_refresh_before_dcutr: record.recommend_refresh_before_dcutr(now),
            first_observed_at: record.first_observed_at,
            last_observed_at: record.last_observed_at,
            confirmed_at: record.confirmed_at,
            expired_at: record.expired_at,
            observation_count: record.observation_count,
            sources: record
                .sources
                .into_iter()
                .map(|source| source.as_str().to_string())
                .collect(),
        }
    }
}

impl From<RelayEndpointStatusRecord> for RelayEndpointStatusSnapshot {
    fn from(record: RelayEndpointStatusRecord) -> Self {
        Self {
            relay_addr: record.relay_addr.to_string(),
            relay_peer_id: record.relay_peer_id.map(|peer_id| peer_id.to_string()),
            transport: record.transport_kind.as_str().to_string(),
            listener_registered: record.listener_registered,
            task_running: record.task_running,
            current_direct_connection_id: record
                .current_direct_connection_id
                .map(|connection_id| connection_id.to_string()),
            last_listener_seen_at: record.last_listener_seen_at,
            last_listener_missing_at: record.last_listener_missing_at,
            last_reservation_accepted_at: record.last_reservation_accepted_at,
            last_reservation_established_at: record.last_reservation_established_at,
            last_reservation_renewed_at: record.last_reservation_renewed_at,
            last_direct_connection_closed_at: record.last_direct_connection_closed_at,
            last_management_action: record
                .last_management_action
                .map(|action| action.as_str().to_string()),
            last_error: record.last_error,
        }
    }
}

impl From<PeerAddressRecord> for PeerAddressSnapshot {
    fn from(record: PeerAddressRecord) -> Self {
        let now = SystemTime::now();
        Self {
            peer_id: record.peer_id.to_string(),
            address: record.address.to_string(),
            transport: record.transport_kind.as_str().to_string(),
            source: record.source.as_str().to_string(),
            freshness: record.freshness(now).as_str().to_string(),
            first_observed_at: record.first_observed_at,
            last_observed_at: record.last_observed_at,
            expired_at: record.expired_at,
            observation_count: record.observation_count,
        }
    }
}

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
    pub policy_state: String,
    pub policy_reason: String,
    pub peer_name: String,
    pub peer_role: String,
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
    #[serde(default)]
    pub remote_port: u16,
}
