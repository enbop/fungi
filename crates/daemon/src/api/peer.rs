use std::{path::PathBuf, time::Duration};

use anyhow::Result;
use fungi_swarm::{ConnectionInfo, PeerConnections, State};
use libp2p::{Multiaddr, PeerId, multiaddr::Protocol, swarm::ConnectionId};

use crate::FungiDaemon;

use super::types::{
    ActiveStreamSnapshot, ConnectionSnapshot, ExternalAddressSnapshot, PeerAddressSnapshot,
    ProtocolStreamCountSnapshot, RelayEndpointStatusSnapshot,
};

impl FungiDaemon {
    fn build_connection_snapshot(
        state: &State,
        peer_id: PeerId,
        direction: &str,
        conn: &ConnectionInfo,
    ) -> ConnectionSnapshot {
        let ping_info = state.connection_ping_info(&conn.connection_id());
        let (last_rtt_ms, last_ping_at) = match ping_info {
            Some(info) => match (info.last_rtt, info.last_rtt_at) {
                (Some(last_rtt), Some(last_rtt_at)) => {
                    (last_rtt.as_millis() as u64, Some(last_rtt_at))
                }
                _ => (0, None),
            },
            None => (0, None),
        };

        let active_streams_by_protocol = state
            .connection_active_stream_protocol_counts(&conn.connection_id())
            .into_iter()
            .map(
                |(protocol_name, stream_count)| ProtocolStreamCountSnapshot {
                    protocol_name,
                    stream_count,
                },
            )
            .collect::<Vec<_>>();
        let active_streams_total = active_streams_by_protocol
            .iter()
            .map(|entry| entry.stream_count)
            .sum();

        let is_relay = is_relay_connection(conn.multiaddr());
        let remote_addr = conn.multiaddr().to_string();
        let governance = state.connection_governance_info(&conn.connection_id());
        ConnectionSnapshot {
            peer_id: peer_id.to_string(),
            connection_id: conn.connection_id().to_string(),
            direction: direction.to_string(),
            is_relay,
            remote_addr,
            last_rtt_ms,
            last_ping_at,
            active_streams_total,
            active_streams_by_protocol,
            policy_state: governance
                .as_ref()
                .map(|info| info.state.as_str().to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            policy_reason: governance.and_then(|info| info.reason).unwrap_or_default(),
        }
    }

    pub fn host_name(&self) -> Option<String> {
        self.config().lock().get_hostname()
    }

    #[cfg(target_os = "android")]
    pub fn init_mobile_device_name(name: String) {
        {
            fungi_util::init_mobile_device_name(name);
        }
    }

    pub fn peer_id(&self) -> String {
        self.swarm_control().local_peer_id().to_string()
    }

    pub fn config_file_path(&self) -> String {
        self.config()
            .lock()
            .config_file_path()
            .to_string_lossy()
            .to_string()
    }

    pub fn add_incoming_allowed_peer(&self, peer_id: PeerId) -> Result<()> {
        // update config and write config file
        let current_config = self.config().lock().clone();
        let updated_config = current_config.add_incoming_allowed_peer(&peer_id)?;
        *self.config().lock() = updated_config;

        // update state
        self.swarm_control()
            .state()
            .incoming_allowed_peers()
            .write()
            .insert(peer_id);
        Ok(())
    }

    pub fn remove_incoming_allowed_peer(&self, peer_id: PeerId) -> Result<()> {
        // update config and write config file
        let current_config = self.config().lock().clone();
        let updated_config = current_config.remove_incoming_allowed_peer(&peer_id)?;
        *self.config().lock() = updated_config;
        // update state
        self.swarm_control()
            .state()
            .incoming_allowed_peers()
            .write()
            .remove(&peer_id);
        // TODO disconnect connected incoming peer
        Ok(())
    }

    pub fn get_file_transfer_service_enabled(&self) -> bool {
        self.config().lock().file_transfer.server.enabled
    }

    pub fn get_file_transfer_service_root_dir(&self) -> PathBuf {
        self.config()
            .lock()
            .file_transfer
            .server
            .shared_root_dir
            .clone()
    }

    pub fn get_peer_connections(&self, peer_id: PeerId) -> Option<PeerConnections> {
        self.swarm_control().state().get_peer_connections(&peer_id)
    }

    pub fn list_external_address_candidates(&self) -> Vec<ExternalAddressSnapshot> {
        self.swarm_control()
            .state()
            .list_external_address_candidates()
            .into_iter()
            .map(Into::into)
            .collect()
    }

    pub fn list_relay_endpoint_statuses(&self) -> Vec<RelayEndpointStatusSnapshot> {
        self.swarm_control()
            .state()
            .list_relay_endpoint_statuses()
            .into_iter()
            .map(Into::into)
            .collect()
    }

    pub fn list_peer_addresses(&self) -> Vec<PeerAddressSnapshot> {
        self.swarm_control()
            .state()
            .list_peer_addresses()
            .into_iter()
            .map(Into::into)
            .collect()
    }

    pub fn list_connections(&self, peer_id: Option<PeerId>) -> Vec<ConnectionSnapshot> {
        let state = self.swarm_control().state();
        let peer_connections = state.peer_connections();
        let peer_connections = peer_connections.lock();

        let mut snapshots = Vec::new();
        for (pid, peer_conn) in peer_connections.iter() {
            if let Some(filter_peer_id) = peer_id
                && *pid != filter_peer_id
            {
                continue;
            }

            for conn in peer_conn.inbound() {
                snapshots.push(Self::build_connection_snapshot(
                    state, *pid, "inbound", conn,
                ));
            }
            for conn in peer_conn.outbound() {
                snapshots.push(Self::build_connection_snapshot(
                    state, *pid, "outbound", conn,
                ));
            }
        }

        snapshots.sort_by(|a, b| {
            a.peer_id
                .cmp(&b.peer_id)
                .then(a.direction.cmp(&b.direction))
                .then(a.connection_id.cmp(&b.connection_id))
        });

        snapshots
    }

    pub fn list_active_streams(&self) -> Vec<ActiveStreamSnapshot> {
        let mut streams = self
            .swarm_control()
            .state()
            .list_active_streams()
            .into_iter()
            .map(|stream| ActiveStreamSnapshot {
                stream_id: stream.stream_id,
                peer_id: stream.peer_id.to_string(),
                connection_id: stream.connection_id.to_string(),
                protocol_name: stream.protocol_name,
                opened_at: stream.opened_at,
            })
            .collect::<Vec<_>>();

        streams.sort_by(|a, b| a.stream_id.cmp(&b.stream_id));
        streams
    }

    pub fn list_active_streams_by_protocol(
        &self,
        protocol_name: String,
    ) -> Vec<ActiveStreamSnapshot> {
        let mut streams = self
            .swarm_control()
            .state()
            .active_streams_by_protocol(&protocol_name)
            .into_iter()
            .map(|stream| ActiveStreamSnapshot {
                stream_id: stream.stream_id,
                peer_id: stream.peer_id.to_string(),
                connection_id: stream.connection_id.to_string(),
                protocol_name: stream.protocol_name,
                opened_at: stream.opened_at,
            })
            .collect::<Vec<_>>();

        streams.sort_by(|a, b| a.stream_id.cmp(&b.stream_id));
        streams
    }

    pub async fn dial_peer_once(&self, peer_id: PeerId) -> Result<()> {
        self.swarm_control()
            .connect(peer_id)
            .await
            .map_err(|e| anyhow::anyhow!("Dial failed: {e}"))?;
        Ok(())
    }

    pub async fn ping_peer_connection(
        &self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        timeout: Duration,
    ) -> Result<std::time::Duration> {
        self.swarm_control()
            .ping_connection(peer_id, connection_id, timeout)
            .await
    }
}

fn is_relay_connection(remote_addr: &Multiaddr) -> bool {
    remote_addr
        .iter()
        .any(|protocol| matches!(protocol, Protocol::P2pCircuit))
}

#[cfg(test)]
mod tests {
    use super::is_relay_connection;

    #[test]
    fn classifies_direct_connection_multiaddr() {
        let direct_addr =
            "/ip4/127.0.0.1/tcp/4001/p2p/12D3KooWQjN7A4xA7bP9g4fC1Qm2nG1k5eYvG8K2Qw4p1D6sZ7Qx"
                .parse()
                .unwrap();

        assert!(!is_relay_connection(&direct_addr));
    }

    #[test]
    fn classifies_relay_connection_multiaddr() {
        let relay_addr = "/ip4/127.0.0.1/tcp/4001/p2p/12D3KooWQjN7A4xA7bP9g4fC1Qm2nG1k5eYvG8K2Qw4p1D6sZ7Qx/p2p-circuit/p2p/12D3KooWKv7kT3rB6uP8a6cS1Lh2dF5mN9qR4xC8vY2pJ7sH3wQz"
            .parse()
            .unwrap();

        assert!(is_relay_connection(&relay_addr));
    }
}
