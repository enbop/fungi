use crate::{AddressFreshness, AddressTransportKind, PeerAddressRecord, PeerAddressSource, State};
use fungi_util::address_policy::{address_bucket, address_within_retention, retain_diverse};
use libp2p::{Multiaddr, PeerId};
use std::time::SystemTime;

const MAX_LEARNED_DIAL_ADDRESSES: usize = 8;
const MAX_STALE_DIAL_ADDRESSES: usize = 2;

#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) enum DialCandidateKind {
    DirectTcp,
    DirectUdp,
}

#[derive(Debug, Clone)]
pub(super) struct DialCandidate {
    pub(super) addr: Multiaddr,
    pub(super) kind: DialCandidateKind,
    pub(super) source: PeerAddressSource,
    pub(super) freshness: AddressFreshness,
    pub(super) last_observed_at: SystemTime,
}

#[derive(Debug, Default)]
pub(super) struct DialPlan {
    pub(super) direct_candidates: Vec<DialCandidate>,
    pub(super) stale_direct_candidates: Vec<DialCandidate>,
    pub(super) skipped_expired: usize,
    pub(super) skipped_non_direct: usize,
}

impl DialPlan {
    pub(super) fn for_peer(state: &State, peer_id: PeerId) -> Self {
        Self::for_peer_at(state, peer_id, SystemTime::now())
    }

    fn for_peer_at(state: &State, peer_id: PeerId, now: SystemTime) -> Self {
        let mut plan = Self::default();

        for record in state.peer_addresses(&peer_id) {
            if !record.source.is_user_managed()
                && !address_within_retention(record.last_observed_at, now)
            {
                plan.skipped_expired += 1;
                continue;
            }

            let Some(candidate) = candidate_from_record(record, now) else {
                plan.skipped_non_direct += 1;
                continue;
            };

            match candidate.freshness {
                AddressFreshness::Fresh | AddressFreshness::Aging => {
                    plan.direct_candidates.push(candidate);
                }
                AddressFreshness::Stale => {
                    plan.stale_direct_candidates.push(candidate);
                }
                AddressFreshness::Expired => {
                    plan.skipped_expired += 1;
                }
            }
        }

        plan.direct_candidates.sort_by(candidate_priority);
        plan.stale_direct_candidates.sort_by(candidate_priority);

        plan
    }

    pub(super) fn direct_addresses(&self) -> Vec<Multiaddr> {
        // Explicit configuration remains usable regardless of age or learned limits.
        let mut configured: Vec<_> = self
            .direct_candidates
            .iter()
            .chain(&self.stale_direct_candidates)
            .filter(|candidate| candidate.source.is_user_managed())
            .collect();
        let mut learned: Vec<_> = self
            .direct_candidates
            .iter()
            .filter(|candidate| !candidate.source.is_user_managed())
            .collect();
        let limit = if learned.is_empty() {
            learned = self
                .stale_direct_candidates
                .iter()
                .filter(|candidate| !candidate.source.is_user_managed())
                .collect();
            MAX_STALE_DIAL_ADDRESSES
        } else {
            MAX_LEARNED_DIAL_ADDRESSES
        };
        retain_diverse(&mut learned, limit, |candidate| {
            address_bucket(&candidate.addr)
        });
        configured.extend(learned);
        configured.sort_by(|a, b| candidate_priority(a, b));
        configured
            .into_iter()
            .map(|candidate| candidate.addr.clone())
            .collect()
    }
}

fn candidate_from_record(record: PeerAddressRecord, now: SystemTime) -> Option<DialCandidate> {
    let kind = match record.transport_kind {
        AddressTransportKind::Tcp => DialCandidateKind::DirectTcp,
        AddressTransportKind::Udp => DialCandidateKind::DirectUdp,
        AddressTransportKind::Relayed | AddressTransportKind::Other => return None,
    };

    Some(DialCandidate {
        addr: record.address.clone(),
        kind,
        source: record.source,
        freshness: record.freshness(now),
        last_observed_at: record.last_observed_at,
    })
}

fn candidate_priority(left: &DialCandidate, right: &DialCandidate) -> std::cmp::Ordering {
    freshness_rank(left.freshness)
        .cmp(&freshness_rank(right.freshness))
        .then(source_rank(left.source).cmp(&source_rank(right.source)))
        .then(right.last_observed_at.cmp(&left.last_observed_at))
        .then(kind_rank(&left.kind).cmp(&kind_rank(&right.kind)))
        .then(left.addr.to_string().cmp(&right.addr.to_string()))
}

fn freshness_rank(freshness: AddressFreshness) -> u8 {
    match freshness {
        AddressFreshness::Fresh => 0,
        AddressFreshness::Aging => 1,
        AddressFreshness::Stale => 2,
        AddressFreshness::Expired => 3,
    }
}

fn source_rank(source: PeerAddressSource) -> u8 {
    match source {
        PeerAddressSource::Mdns => 0,
        PeerAddressSource::Identify => 1,
        PeerAddressSource::DeviceConfig => 2,
        PeerAddressSource::DirectCache => 3,
        PeerAddressSource::Manual => 4,
        PeerAddressSource::RelayDerived => 5,
        PeerAddressSource::AutoNat => 6,
        PeerAddressSource::Other => 7,
    }
}

fn kind_rank(kind: &DialCandidateKind) -> u8 {
    match kind {
        DialCandidateKind::DirectUdp => 0,
        DialCandidateKind::DirectTcp => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PeerAddressObservation;

    #[test]
    fn dial_plan_orders_fresh_mdns_before_identify_and_device_config() {
        let peer_id = PeerId::random();
        let state = State::default();

        let device_config_addr: Multiaddr = format!("/ip4/198.51.100.8/tcp/4001/p2p/{peer_id}")
            .parse()
            .unwrap();
        let identify_addr: Multiaddr = format!("/ip4/203.0.113.8/tcp/4001/p2p/{peer_id}")
            .parse()
            .unwrap();
        let mdns_addr: Multiaddr = format!("/ip4/192.168.1.8/tcp/4001/p2p/{peer_id}")
            .parse()
            .unwrap();

        assert_eq!(
            state.record_peer_address(
                peer_id,
                device_config_addr.clone(),
                PeerAddressSource::DeviceConfig
            ),
            PeerAddressObservation::New
        );
        assert_eq!(
            state.record_peer_address(peer_id, identify_addr.clone(), PeerAddressSource::Identify),
            PeerAddressObservation::New
        );
        assert_eq!(
            state.record_peer_address(peer_id, mdns_addr.clone(), PeerAddressSource::Mdns),
            PeerAddressObservation::New
        );

        let plan = DialPlan::for_peer(&state, peer_id);

        let expected_mdns_addr: Multiaddr = "/ip4/192.168.1.8/tcp/4001".parse().unwrap();
        let expected_identify_addr: Multiaddr = "/ip4/203.0.113.8/tcp/4001".parse().unwrap();
        let expected_device_config_addr: Multiaddr = "/ip4/198.51.100.8/tcp/4001".parse().unwrap();

        assert_eq!(
            plan.direct_addresses(),
            vec![
                expected_mdns_addr,
                expected_identify_addr,
                expected_device_config_addr
            ]
        );
    }

    #[test]
    fn dial_plan_uses_stale_direct_addresses_when_no_fresher_addresses_exist() {
        let peer_id = PeerId::random();
        let stale_addr: Multiaddr = format!("/ip4/198.51.100.9/tcp/4001/p2p/{peer_id}")
            .parse()
            .unwrap();
        let candidate = DialCandidate {
            addr: stale_addr.clone(),
            kind: DialCandidateKind::DirectTcp,
            source: PeerAddressSource::DeviceConfig,
            freshness: AddressFreshness::Stale,
            last_observed_at: SystemTime::now(),
        };
        let plan = DialPlan {
            direct_candidates: Vec::new(),
            stale_direct_candidates: vec![candidate],
            skipped_expired: 0,
            skipped_non_direct: 0,
        };

        assert!(plan.direct_candidates.is_empty());
        assert_eq!(plan.stale_direct_candidates.len(), 1);
        assert_eq!(plan.direct_addresses(), vec![stale_addr]);
    }
    fn observe(
        state: &State,
        peer: PeerId,
        address: &str,
        source: PeerAddressSource,
        at: SystemTime,
    ) {
        state.restore_peer_address_record(peer, address.parse().unwrap(), source, at, at, 1);
    }

    #[test]
    fn bounds_140_stale_addresses_and_drops_expired_memory_without_restart() {
        use fungi_util::address_policy::DIRECT_ADDRESS_RETENTION;
        use std::time::Duration;
        let peer = PeerId::random();
        let state = State::default();
        let now = SystemTime::now();
        for index in 0..140 {
            observe(
                &state,
                peer,
                &format!("/ip4/192.168.1.145/udp/{}/quic-v1", 4000 + index),
                PeerAddressSource::DirectCache,
                now - Duration::from_secs(3600 + 140 - index),
            );
        }
        let addresses = DialPlan::for_peer_at(&state, peer, now).direct_addresses();
        assert_eq!(
            addresses,
            vec![
                "/ip4/192.168.1.145/udp/4139/quic-v1"
                    .parse::<Multiaddr>()
                    .unwrap(),
                "/ip4/192.168.1.145/udp/4138/quic-v1"
                    .parse::<Multiaddr>()
                    .unwrap(),
            ]
        );
        let expired = DialPlan::for_peer_at(&state, peer, now + DIRECT_ADDRESS_RETENTION);
        assert!(expired.direct_addresses().is_empty());
        assert_eq!(expired.skipped_expired, 140);
        assert_eq!(state.peer_addresses(&peer).len(), 140);
    }

    #[test]
    fn fresh_discovery_is_bounded_diverse_and_keeps_explicit_addresses() {
        use fungi_util::address_policy::DIRECT_ADDRESS_RETENTION;
        use std::time::Duration;
        let peer = PeerId::random();
        let state = State::default();
        let now = SystemTime::now();
        for index in 1..=20 {
            observe(
                &state,
                peer,
                &format!("/ip6/2001:db8::{index}/udp/4001/quic-v1"),
                PeerAddressSource::Mdns,
                now,
            );
        }
        let ipv4 = "/ip4/192.168.1.145/udp/5001/quic-v1";
        let tcp = "/ip6/2001:db8::ffff/tcp/5002";
        observe(&state, peer, ipv4, PeerAddressSource::Mdns, now);
        observe(&state, peer, tcp, PeerAddressSource::Mdns, now);
        // Explicit fixed addresses remain available even when older than cache TTL.
        for port in 6000..6010 {
            observe(
                &state,
                peer,
                &format!("/ip4/192.168.1.146/tcp/{port}"),
                PeerAddressSource::DeviceConfig,
                now - DIRECT_ADDRESS_RETENTION - Duration::from_secs(1),
            );
        }
        let stale = "/ip4/192.168.1.145/tcp/9";
        observe(
            &state,
            peer,
            stale,
            PeerAddressSource::DirectCache,
            now - Duration::from_secs(3600),
        );
        let unrelated_peer = PeerId::random();
        observe(
            &state,
            unrelated_peer,
            "/ip4/192.168.1.200/tcp/8",
            PeerAddressSource::Mdns,
            now,
        );

        let addresses = DialPlan::for_peer_at(&state, peer, now).direct_addresses();
        assert_eq!(addresses.len(), MAX_LEARNED_DIAL_ADDRESSES + 10);
        assert!(addresses.contains(&ipv4.parse().unwrap()));
        assert!(addresses.contains(&tcp.parse().unwrap()));
        assert!(!addresses.contains(&stale.parse().unwrap()));
        assert_eq!(
            addresses
                .iter()
                .filter(|a| a.to_string().contains("192.168.1.146"))
                .count(),
            10
        );
    }
}
