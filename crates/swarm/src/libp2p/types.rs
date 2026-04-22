use crate::ConnectionRecord;
use libp2p::swarm::ConnectionId;
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone, Copy, Default)]
pub enum ConnectionSelectionStrategy {
    #[default]
    PreferDirect,
    PreferRelay,
}

pub(crate) trait ConnectionRecordSliceExt {
    fn sort_by_strategy(&mut self, strategy: ConnectionSelectionStrategy);
}

impl ConnectionRecordSliceExt for [ConnectionRecord] {
    fn sort_by_strategy(&mut self, strategy: ConnectionSelectionStrategy) {
        self.sort_by(|a, b| match strategy {
            ConnectionSelectionStrategy::PreferDirect => a
                .is_relay()
                .cmp(&b.is_relay())
                .then(
                    established_at_key(Some(a.established_at))
                        .cmp(&established_at_key(Some(b.established_at))),
                )
                .then(conn_id_key(a.connection_id).cmp(&conn_id_key(b.connection_id))),
            ConnectionSelectionStrategy::PreferRelay => b
                .is_relay()
                .cmp(&a.is_relay())
                .then(
                    established_at_key(Some(a.established_at))
                        .cmp(&established_at_key(Some(b.established_at))),
                )
                .then(conn_id_key(a.connection_id).cmp(&conn_id_key(b.connection_id))),
        });
    }
}

fn conn_id_key(id: ConnectionId) -> u64 {
    let serialized = id.to_string();
    if let Ok(value) = serialized.parse::<u64>() {
        return value;
    }

    let digits: String = serialized.chars().filter(|c| c.is_ascii_digit()).collect();
    digits.parse::<u64>().unwrap_or(u64::MAX)
}

fn established_at_key(established_at: Option<SystemTime>) -> Duration {
    established_at
        .and_then(|ts| ts.duration_since(SystemTime::UNIX_EPOCH).ok())
        .unwrap_or(Duration::MAX)
}
