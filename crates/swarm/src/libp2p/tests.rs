use super::{ConnectionSelectionStrategy, SelectedConnection, SwarmControl};
use crate::{ConnectionDirection, ConnectionGovernanceInfo, ConnectionGovernanceState};
use libp2p::{Multiaddr, swarm::ConnectionId};
use std::{
    collections::HashMap,
    time::{Duration, SystemTime},
};

use super::{
    governance::{CONNECTION_GOVERNANCE_GRACE_PERIOD, GovernanceAction, next_governance_action},
    relay::{
        RelayEndpoint, RelayPeers, RelayTransportKind, multiaddr_starts_with, relay_retry_delay,
        relay_transport_kind,
    },
};

#[test]
fn relay_listener_match_accepts_confirmed_listener_addr() {
    let relay_endpoint = RelayEndpoint::new(
        "/ip4/160.16.206.21/udp/30001/quic-v1/p2p/16Uiu2HAmGXFS6aYsKKYRkEDo1tNigZKN8TAYrsfSnEdC5sZLNkiE"
            .parse()
            .unwrap(),
    );
    let confirmed_listener = "/ip4/160.16.206.21/udp/30001/quic-v1/p2p/16Uiu2HAmGXFS6aYsKKYRkEDo1tNigZKN8TAYrsfSnEdC5sZLNkiE/p2p-circuit/p2p/16Uiu2HAmGXFS6aYsKKYRkEDo1tNigZKN8TAYrsfSnEdC5sZLNkiE"
        .parse()
        .unwrap();

    assert!(relay_endpoint.matches_listener(&confirmed_listener));
}

#[test]
fn multiaddr_prefix_match_rejects_different_transport() {
    let relay_prefix = "/ip4/160.16.206.21/udp/30001/quic-v1/p2p/16Uiu2HAmGXFS6aYsKKYRkEDo1tNigZKN8TAYrsfSnEdC5sZLNkiE/p2p-circuit"
        .parse()
        .unwrap();
    let tcp_listener = "/ip4/160.16.206.21/tcp/30001/p2p/16Uiu2HAmGXFS6aYsKKYRkEDo1tNigZKN8TAYrsfSnEdC5sZLNkiE/p2p-circuit/p2p/16Uiu2HAmGXFS6aYsKKYRkEDo1tNigZKN8TAYrsfSnEdC5sZLNkiE"
        .parse()
        .unwrap();

    assert!(!multiaddr_starts_with(&tcp_listener, &relay_prefix));
}

#[test]
fn relay_transport_kind_detects_quic_as_udp() {
    let addr = "/ip4/160.16.206.21/udp/30001/quic-v1/p2p/16Uiu2HAmGXFS6aYsKKYRkEDo1tNigZKN8TAYrsfSnEdC5sZLNkiE"
        .parse()
        .unwrap();

    assert_eq!(relay_transport_kind(&addr), Some(RelayTransportKind::Udp));
}

#[test]
fn relay_endpoint_matches_transport_by_protocol_kind() {
    let relay_endpoint = RelayEndpoint::new(
        "/ip4/160.16.206.21/tcp/30001/p2p/16Uiu2HAmGXFS6aYsKKYRkEDo1tNigZKN8TAYrsfSnEdC5sZLNkiE"
            .parse()
            .unwrap(),
    );
    let tcp_remote =
        "/ip4/160.16.206.21/tcp/30001/p2p/16Uiu2HAmGXFS6aYsKKYRkEDo1tNigZKN8TAYrsfSnEdC5sZLNkiE"
            .parse()
            .unwrap();
    let udp_remote = "/ip4/160.16.206.21/udp/30001/quic-v1/p2p/16Uiu2HAmGXFS6aYsKKYRkEDo1tNigZKN8TAYrsfSnEdC5sZLNkiE"
        .parse()
        .unwrap();

    assert!(relay_endpoint.matches_transport(&tcp_remote));
    assert!(!relay_endpoint.matches_transport(&udp_remote));
}

#[test]
fn relay_udp_refresh_plan_deduplicates_udp_addresses_and_prioritizes_preferred_relay() {
    let preferred_peer = libp2p::identity::Keypair::generate_ed25519()
        .public()
        .to_peer_id();
    let other_peer = libp2p::identity::Keypair::generate_ed25519()
        .public()
        .to_peer_id();

    let preferred_udp: Multiaddr = format!("/ip4/127.0.0.1/udp/30002/quic-v1/p2p/{preferred_peer}")
        .parse()
        .unwrap();
    let other_udp: Multiaddr = format!("/ip4/127.0.0.1/udp/30001/quic-v1/p2p/{other_peer}")
        .parse()
        .unwrap();
    let other_tcp: Multiaddr = format!("/ip4/127.0.0.1/tcp/30001/p2p/{other_peer}")
        .parse()
        .unwrap();

    let relay_peers = RelayPeers::new(vec![
        other_tcp,
        other_udp.clone(),
        other_udp.clone(),
        preferred_udp.clone(),
    ]);

    let plan = relay_peers.udp_refresh_plan(Some(preferred_peer));

    assert!(plan.preferred_relay_matched);
    assert_eq!(plan.skipped_duplicate_addr, 1);
    assert_eq!(plan.targets.len(), 2);
    assert_eq!(plan.targets[0].peer_id, preferred_peer);
    assert_eq!(plan.targets[0].addresses, vec![preferred_udp]);
    assert_eq!(plan.targets[1].peer_id, other_peer);
    assert_eq!(plan.targets[1].addresses, vec![other_udp]);
}

#[test]
fn relay_circuit_addresses_only_use_tcp_relay_endpoints() {
    let relay_peer = libp2p::identity::Keypair::generate_ed25519()
        .public()
        .to_peer_id();
    let target_peer = libp2p::identity::Keypair::generate_ed25519()
        .public()
        .to_peer_id();

    let tcp_addr: Multiaddr = format!("/ip4/127.0.0.1/tcp/30001/p2p/{relay_peer}")
        .parse()
        .unwrap();
    let udp_addr: Multiaddr = format!("/ip4/127.0.0.1/udp/30001/quic-v1/p2p/{relay_peer}")
        .parse()
        .unwrap();

    let relay_peers = RelayPeers::new(vec![tcp_addr.clone(), udp_addr]);

    assert_eq!(relay_peers.peer_ids(), &[relay_peer]);
    assert_eq!(
        relay_peers.circuit_addresses_for_target(target_peer),
        vec![super::relay::peer_addr_with_relay(target_peer, tcp_addr)]
    );
}

#[test]
fn relay_retry_delay_switches_from_fast_backoff_to_persistent_cap() {
    assert_eq!(relay_retry_delay(1), Duration::from_millis(500));
    assert_eq!(relay_retry_delay(2), Duration::from_secs(1));
    assert_eq!(relay_retry_delay(3), Duration::from_secs(2));
    assert_eq!(relay_retry_delay(4), Duration::from_secs(4));
    assert_eq!(relay_retry_delay(5), Duration::from_secs(60));
    assert_eq!(relay_retry_delay(u32::MAX), Duration::from_secs(60));
}

fn selected_connection(
    connection_id: usize,
    remote_addr: &str,
    is_relay: bool,
    last_rtt_ms: Option<u64>,
) -> SelectedConnection {
    SelectedConnection {
        connection_id: ConnectionId::new_unchecked(connection_id),
        direction: ConnectionDirection::Outbound,
        remote_addr: remote_addr.parse::<Multiaddr>().unwrap(),
        is_relay,
        last_rtt: last_rtt_ms.map(Duration::from_millis),
        active_stream_count: 0,
        established_at: None,
    }
}

#[test]
fn sort_selected_connections_prefers_active_streams_before_rtt() {
    let mut selected = vec![
        SelectedConnection {
            active_stream_count: 0,
            ..selected_connection(8, "/ip4/1.1.1.1/tcp/4002", false, Some(10))
        },
        SelectedConnection {
            active_stream_count: 2,
            ..selected_connection(6, "/ip4/1.1.1.1/tcp/4001", false, Some(30))
        },
    ];

    SwarmControl::sort_selected_connections(
        ConnectionSelectionStrategy::PreferDirect,
        &mut selected,
    );

    assert_eq!(selected[0].connection_id, ConnectionId::new_unchecked(6));
}

#[test]
fn sort_selected_connections_prefers_earlier_established_when_other_signals_match() {
    let earlier = SystemTime::UNIX_EPOCH + Duration::from_secs(10);
    let later = SystemTime::UNIX_EPOCH + Duration::from_secs(20);
    let mut selected = vec![
        SelectedConnection {
            established_at: Some(later),
            ..selected_connection(8, "/ip4/1.1.1.1/tcp/4002", false, Some(30))
        },
        SelectedConnection {
            established_at: Some(earlier),
            ..selected_connection(6, "/ip4/1.1.1.1/tcp/4001", false, Some(30))
        },
    ];

    SwarmControl::sort_selected_connections(
        ConnectionSelectionStrategy::PreferDirect,
        &mut selected,
    );

    assert_eq!(selected[0].connection_id, ConnectionId::new_unchecked(6));
}

#[test]
fn closure_plan_prefers_direct_and_closes_idle_relay() {
    let mut selected = vec![
        selected_connection(9, "/ip4/1.1.1.1/tcp/4001/p2p-circuit", true, Some(20)),
        selected_connection(4, "/ip4/1.1.1.1/tcp/4001", false, Some(50)),
    ];
    let active_stream_counts = HashMap::from([
        (ConnectionId::new_unchecked(9), 0usize),
        (ConnectionId::new_unchecked(4), 0usize),
    ]);
    let governance_info = HashMap::from([
        (
            ConnectionId::new_unchecked(9),
            ConnectionGovernanceInfo {
                state: ConnectionGovernanceState::Closing,
                reason: Some("lower-priority-than-connection-4".to_string()),
                changed_at: Some(SystemTime::now()),
            },
        ),
        (
            ConnectionId::new_unchecked(4),
            ConnectionGovernanceInfo {
                state: ConnectionGovernanceState::Recommended,
                reason: Some("selected-by-prefer-direct-baseline".to_string()),
                changed_at: Some(SystemTime::now()),
            },
        ),
    ]);

    let plan = SwarmControl::build_connection_closure_plan(
        ConnectionSelectionStrategy::PreferDirect,
        &mut selected,
        &active_stream_counts,
        &governance_info,
        SystemTime::now(),
    );

    assert_eq!(plan.len(), 1);
    assert_eq!(plan[0].connection_id, ConnectionId::new_unchecked(9));
    assert_eq!(
        plan[0].recommended_connection_id,
        ConnectionId::new_unchecked(4)
    );
    assert_eq!(plan[0].action, GovernanceAction::CloseNow);
}

#[test]
fn closure_plan_keeps_deprecated_connection_when_stream_is_active() {
    let mut selected = vec![
        selected_connection(8, "/ip4/1.1.1.1/tcp/4002", false, Some(90)),
        selected_connection(6, "/ip4/1.1.1.1/tcp/4001", false, Some(15)),
    ];
    let active_stream_counts = HashMap::from([
        (ConnectionId::new_unchecked(8), 2usize),
        (ConnectionId::new_unchecked(6), 0usize),
    ]);
    let governance_info = HashMap::from([
        (
            ConnectionId::new_unchecked(8),
            ConnectionGovernanceInfo {
                state: ConnectionGovernanceState::Closing,
                reason: Some("lower-priority-than-connection-6".to_string()),
                changed_at: Some(SystemTime::now()),
            },
        ),
        (
            ConnectionId::new_unchecked(6),
            ConnectionGovernanceInfo {
                state: ConnectionGovernanceState::Recommended,
                reason: Some("selected-by-prefer-direct-baseline".to_string()),
                changed_at: Some(SystemTime::now()),
            },
        ),
    ]);

    let plan = SwarmControl::build_connection_closure_plan(
        ConnectionSelectionStrategy::PreferDirect,
        &mut selected,
        &active_stream_counts,
        &governance_info,
        SystemTime::now(),
    );

    assert_eq!(plan.len(), 1);
    assert_eq!(plan[0].action, GovernanceAction::MarkDeprecated);
}

#[test]
fn next_governance_action_respects_grace_period_before_closing() {
    let now = SystemTime::now();
    let current = ConnectionGovernanceInfo {
        state: ConnectionGovernanceState::Deprecated,
        reason: Some("lower-priority-than-connection-4".to_string()),
        changed_at: Some(now),
    };

    let action = next_governance_action(Some(&current), "lower-priority-than-connection-4", 0, now);

    assert_eq!(action, None);
}

#[test]
fn next_governance_action_transitions_to_closing_and_then_close() {
    let now = SystemTime::now();
    let deprecated = ConnectionGovernanceInfo {
        state: ConnectionGovernanceState::Deprecated,
        reason: Some("lower-priority-than-connection-4".to_string()),
        changed_at: Some(now - CONNECTION_GOVERNANCE_GRACE_PERIOD),
    };
    let closing = ConnectionGovernanceInfo {
        state: ConnectionGovernanceState::Closing,
        reason: Some("lower-priority-than-connection-4".to_string()),
        changed_at: Some(now),
    };

    assert_eq!(
        next_governance_action(
            Some(&deprecated),
            "lower-priority-than-connection-4",
            0,
            now,
        ),
        Some(GovernanceAction::MarkClosing)
    );
    assert_eq!(
        next_governance_action(Some(&closing), "lower-priority-than-connection-4", 0, now),
        Some(GovernanceAction::CloseNow)
    );
}
