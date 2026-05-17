use anyhow::Result;
use fungi_swarm::{ConnectionDirection, ConnectionRecord};
use libp2p::{Multiaddr, PeerId, StreamProtocol, multiaddr::Protocol};

use crate::FungiDaemon;

use super::types::{
    ActiveStreamSnapshot, ConnectionSnapshot, ExternalAddressSnapshot, PeerAddressSnapshot,
    ProtocolStreamCountSnapshot, RelayEndpointStatusSnapshot,
};

impl FungiDaemon {
    fn build_connection_snapshot(
        &self,
        peer_id: PeerId,
        conn: &ConnectionRecord,
    ) -> ConnectionSnapshot {
        let (last_rtt_ms, last_ping_at) =
            match (conn.ping_info.last_rtt, conn.ping_info.last_rtt_at) {
                (Some(last_rtt), Some(last_rtt_at)) => {
                    (last_rtt.as_millis() as u64, Some(last_rtt_at))
                }
                _ => (0, None),
            };

        let active_streams_by_protocol = self
            .swarm_control()
            .state()
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
        let peer_name = self
            .devices_get_peer(peer_id)
            .and_then(|peer| peer.name)
            .unwrap_or_default();
        let peer_role = if self.is_configured_relay_peer(peer_id) {
            "relay-carrier"
        } else if is_relay {
            "relayed-peer"
        } else {
            "peer"
        }
        .to_string();
        ConnectionSnapshot {
            peer_id: peer_id.to_string(),
            connection_id: conn.connection_id().to_string(),
            direction: match conn.direction {
                ConnectionDirection::Inbound => "inbound",
                ConnectionDirection::Outbound => "outbound",
            }
            .to_string(),
            is_relay,
            remote_addr,
            last_rtt_ms,
            last_ping_at,
            active_streams_total,
            active_streams_by_protocol,
            policy_state: conn.governance.state.as_str().to_string(),
            policy_reason: conn.governance.reason.clone().unwrap_or_default(),
            peer_name,
            peer_role,
        }
    }

    fn is_configured_relay_peer(&self, peer_id: PeerId) -> bool {
        self.swarm_control()
            .state()
            .list_relay_endpoint_statuses()
            .into_iter()
            .any(|status| status.relay_peer_id == Some(peer_id))
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

    pub fn trust_device(&self, peer_id: PeerId) -> Result<()> {
        let current_config = self.trusted_devices().lock().clone();
        let updated_config = current_config.add_trusted_device(&peer_id)?;
        *self.trusted_devices().lock() = updated_config;

        self.swarm_control()
            .state()
            .incoming_allowed_peers()
            .write()
            .insert(peer_id);
        Ok(())
    }

    pub fn untrust_device(&self, peer_id: PeerId) -> Result<()> {
        let current_config = self.trusted_devices().lock().clone();
        let updated_config = current_config.remove_trusted_device(&peer_id)?;
        *self.trusted_devices().lock() = updated_config;

        self.swarm_control()
            .state()
            .incoming_allowed_peers()
            .write()
            .remove(&peer_id);
        // TODO disconnect connected incoming peer
        Ok(())
    }

    pub fn get_peer_connections(&self, peer_id: PeerId) -> Option<Vec<ConnectionRecord>> {
        let connections = self
            .swarm_control()
            .state()
            .get_connections_by_peer_id(&peer_id);
        if connections.is_empty() {
            None
        } else {
            Some(connections)
        }
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

        let mut snapshots = Vec::new();
        for pid in state.connected_peer_ids() {
            if let Some(filter_peer_id) = peer_id
                && pid != filter_peer_id
            {
                continue;
            }

            for conn in state.get_connections_by_peer_id(&pid) {
                snapshots.push(self.build_connection_snapshot(pid, &conn));
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
                protocol_name: stream.protocol.to_string(),
                opened_at: stream.opened_at,
            })
            .collect::<Vec<_>>();

        streams.sort_by(|a, b| a.stream_id.cmp(&b.stream_id));
        streams
    }

    pub fn list_active_streams_by_protocol(
        &self,
        protocol: StreamProtocol,
    ) -> Vec<ActiveStreamSnapshot> {
        let mut streams = self
            .swarm_control()
            .state()
            .active_streams_by_protocol(&protocol)
            .into_iter()
            .map(|stream| ActiveStreamSnapshot {
                stream_id: stream.stream_id,
                peer_id: stream.peer_id.to_string(),
                connection_id: stream.connection_id.to_string(),
                protocol_name: stream.protocol.to_string(),
                opened_at: stream.opened_at,
            })
            .collect::<Vec<_>>();

        streams.sort_by(|a, b| a.stream_id.cmp(&b.stream_id));
        streams
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
