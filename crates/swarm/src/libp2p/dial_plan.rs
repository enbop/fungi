use crate::{AddressFreshness, AddressTransportKind, PeerAddressRecord, PeerAddressSource, State};
use libp2p::{Multiaddr, PeerId};
use std::time::SystemTime;

#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) enum DialCandidateKind {
    DirectTcp,
    DirectUdp,
}

impl DialCandidateKind {
    fn as_str(&self) -> &'static str {
        match self {
            DialCandidateKind::DirectTcp => "direct-tcp",
            DialCandidateKind::DirectUdp => "direct-udp",
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct DialCandidate {
    pub(super) addr: Multiaddr,
    pub(super) kind: DialCandidateKind,
    pub(super) source: PeerAddressSource,
    pub(super) freshness: AddressFreshness,
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
        let now = SystemTime::now();
        let mut plan = Self::default();

        for record in state.list_peer_addresses() {
            if record.peer_id != peer_id {
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
        let candidates = if self.direct_candidates.is_empty() {
            &self.stale_direct_candidates
        } else {
            &self.direct_candidates
        };

        candidates
            .iter()
            .map(|candidate| candidate.addr.clone())
            .collect()
    }

    pub(super) fn using_stale_direct_addresses(&self) -> bool {
        self.direct_candidates.is_empty() && !self.stale_direct_candidates.is_empty()
    }

    pub(super) fn direct_summary(&self) -> String {
        let candidates = if self.direct_candidates.is_empty() {
            &self.stale_direct_candidates
        } else {
            &self.direct_candidates
        };

        if candidates.is_empty() {
            return format!(
                "none (skipped_expired={}, skipped_non_direct={})",
                self.skipped_expired, self.skipped_non_direct
            );
        }

        candidates
            .iter()
            .map(|candidate| {
                format!(
                    "{} source={} freshness={} addr={}",
                    candidate.kind.as_str(),
                    candidate.source.as_str(),
                    candidate.freshness.as_str(),
                    candidate.addr
                )
            })
            .collect::<Vec<_>>()
            .join("; ")
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
    })
}

fn candidate_priority(left: &DialCandidate, right: &DialCandidate) -> std::cmp::Ordering {
    freshness_rank(left.freshness)
        .cmp(&freshness_rank(right.freshness))
        .then(source_rank(left.source).cmp(&source_rank(right.source)))
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
        PeerAddressSource::AddressBook => 2,
        PeerAddressSource::Manual => 3,
        PeerAddressSource::RelayDerived => 4,
        PeerAddressSource::AutoNat => 5,
        PeerAddressSource::Other => 6,
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
    fn dial_plan_orders_fresh_mdns_before_identify_and_address_book() {
        let peer_id = PeerId::random();
        let state = State::default();

        let address_book_addr: Multiaddr = format!("/ip4/198.51.100.8/tcp/4001/p2p/{peer_id}")
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
                address_book_addr.clone(),
                PeerAddressSource::AddressBook
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
        let expected_address_book_addr: Multiaddr = "/ip4/198.51.100.8/tcp/4001".parse().unwrap();

        assert_eq!(
            plan.direct_addresses(),
            vec![
                expected_mdns_addr,
                expected_identify_addr,
                expected_address_book_addr
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
            source: PeerAddressSource::AddressBook,
            freshness: AddressFreshness::Stale,
        };
        let plan = DialPlan {
            direct_candidates: Vec::new(),
            stale_direct_candidates: vec![candidate],
            skipped_expired: 0,
            skipped_non_direct: 0,
        };

        assert!(plan.using_stale_direct_addresses());
        assert_eq!(plan.direct_addresses(), vec![stale_addr]);
    }
}
