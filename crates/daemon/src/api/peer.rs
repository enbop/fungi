use std::{path::PathBuf, time::Duration};

use anyhow::Result;
use fungi_swarm::{ConnectionInfo, PeerConnections, State};
use libp2p::{PeerId, swarm::ConnectionId};

use crate::FungiDaemon;

use super::types::{
    ActiveStreamSnapshot, ConnectionSnapshot, ExternalAddressSnapshot, PeerAddressSnapshot,
    ProtocolStreamCountSnapshot, RelayEndpointStatusSnapshot,
};

fn connection_id_sort_key(connection_id: &str) -> u64 {
    if let Ok(value) = connection_id.parse::<u64>() {
        return value;
    }

    let digits: String = connection_id
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect();
    digits.parse::<u64>().unwrap_or(u64::MAX)
}

fn connection_rtt_sort_key(last_rtt_ms: u64, last_ping_at: Option<std::time::SystemTime>) -> u64 {
    if last_ping_at.is_some() {
        last_rtt_ms
    } else {
        u64::MAX
    }
}

fn apply_connection_policy(snapshots: &mut [ConnectionSnapshot]) {
    snapshots.sort_by(|left, right| {
        left.peer_id
            .cmp(&right.peer_id)
            .then(left.is_relay.cmp(&right.is_relay))
            .then(
                connection_rtt_sort_key(left.last_rtt_ms, left.last_ping_at).cmp(
                    &connection_rtt_sort_key(right.last_rtt_ms, right.last_ping_at),
                ),
            )
            .then(
                connection_id_sort_key(&left.connection_id)
                    .cmp(&connection_id_sort_key(&right.connection_id)),
            )
    });

    let mut start = 0usize;
    while start < snapshots.len() {
        let peer_id = snapshots[start].peer_id.clone();
        let mut end = start + 1;
        while end < snapshots.len() && snapshots[end].peer_id == peer_id {
            end += 1;
        }

        let recommended_id = snapshots[start].connection_id.clone();
        for (offset, snapshot) in snapshots[start..end].iter_mut().enumerate() {
            if offset == 0 {
                snapshot.policy_state = "recommended".to_string();
                snapshot.policy_reason = "selected-by-prefer-direct-baseline".to_string();
            } else {
                snapshot.policy_state = "deprecated".to_string();
                snapshot.policy_reason =
                    format!("lower-priority-than-connection-{}", recommended_id);
            }
        }

        start = end;
    }

    snapshots.sort_by(|a, b| {
        a.peer_id
            .cmp(&b.peer_id)
            .then(a.direction.cmp(&b.direction))
            .then(a.connection_id.cmp(&b.connection_id))
    });
}

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

        let remote_addr = conn.multiaddr().to_string();
        ConnectionSnapshot {
            peer_id: peer_id.to_string(),
            connection_id: conn.connection_id().to_string(),
            direction: direction.to_string(),
            is_relay: remote_addr.contains("/p2p-circuit"),
            remote_addr,
            last_rtt_ms,
            last_ping_at,
            active_streams_total,
            active_streams_by_protocol,
            policy_state: "unknown".to_string(),
            policy_reason: String::new(),
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

        apply_connection_policy(&mut snapshots);

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

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(
        peer_id: &str,
        connection_id: &str,
        is_relay: bool,
        last_rtt_ms: u64,
    ) -> ConnectionSnapshot {
        ConnectionSnapshot {
            peer_id: peer_id.to_string(),
            connection_id: connection_id.to_string(),
            direction: "outbound".to_string(),
            remote_addr: if is_relay {
                "/ip4/1.1.1.1/tcp/4001/p2p-circuit".to_string()
            } else {
                "/ip4/1.1.1.1/tcp/4001".to_string()
            },
            is_relay,
            last_rtt_ms,
            last_ping_at: if last_rtt_ms == 0 {
                None
            } else {
                Some(std::time::SystemTime::now())
            },
            active_streams_total: 0,
            active_streams_by_protocol: Vec::new(),
            policy_state: String::new(),
            policy_reason: String::new(),
        }
    }

    #[test]
    fn apply_connection_policy_prefers_direct_before_relay() {
        let mut snapshots = vec![
            snapshot("peer-a", "9", true, 20),
            snapshot("peer-a", "4", false, 50),
        ];

        apply_connection_policy(&mut snapshots);

        let direct = snapshots
            .iter()
            .find(|entry| entry.connection_id == "4")
            .unwrap();
        let relay = snapshots
            .iter()
            .find(|entry| entry.connection_id == "9")
            .unwrap();
        assert_eq!(direct.policy_state, "recommended");
        assert_eq!(relay.policy_state, "deprecated");
        assert_eq!(relay.policy_reason, "lower-priority-than-connection-4");
    }

    #[test]
    fn apply_connection_policy_prefers_lower_rtt_within_same_class() {
        let mut snapshots = vec![
            snapshot("peer-a", "8", false, 90),
            snapshot("peer-a", "6", false, 15),
        ];

        apply_connection_policy(&mut snapshots);

        let preferred = snapshots
            .iter()
            .find(|entry| entry.connection_id == "6")
            .unwrap();
        let deprecated = snapshots
            .iter()
            .find(|entry| entry.connection_id == "8")
            .unwrap();
        assert_eq!(preferred.policy_state, "recommended");
        assert_eq!(deprecated.policy_state, "deprecated");
    }
}
