//! Shared bounds for historical direct addresses and in-memory dial selection.

use multiaddr::{Multiaddr, Protocol};
use std::time::{Duration, SystemTime};

pub const MAX_CACHED_ADDRESSES_PER_PEER: usize = 8;
pub const DIRECT_ADDRESS_RETENTION: Duration = Duration::from_secs(7 * 24 * 60 * 60);

pub fn address_within_retention(observed_at: SystemTime, now: SystemTime) -> bool {
    // A clock adjustment must not discard otherwise usable addresses.
    now.duration_since(observed_at).unwrap_or_default() <= DIRECT_ADDRESS_RETENTION
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressFamily {
    Unknown,
    Ipv4,
    Ipv6,
    Dns,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Unknown,
    Tcp,
    Udp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AddressBucket {
    pub family: AddressFamily,
    pub transport: Transport,
}

/// Address family and transport are independent: preserve both TCP and QUIC options.
/// DNS and unknown families have their own buckets instead of masquerading as IPv4.
pub fn address_bucket(address: &Multiaddr) -> AddressBucket {
    let mut family = AddressFamily::Unknown;
    let mut transport = Transport::Unknown;
    for protocol in address.iter() {
        match protocol {
            Protocol::Ip4(_) => family = AddressFamily::Ipv4,
            Protocol::Ip6(_) => family = AddressFamily::Ipv6,
            Protocol::Dns(_) | Protocol::Dns4(_) | Protocol::Dns6(_) => family = AddressFamily::Dns,
            Protocol::Tcp(_) => transport = Transport::Tcp,
            Protocol::Udp(_) => transport = Transport::Udp,
            _ => {}
        }
    }
    AddressBucket { family, transport }
}

/// Keep candidates from a priority-sorted list, taking one per bucket each round.
/// Remaining capacity can all go to one bucket; the retained order is unchanged.
pub fn retain_diverse<T, K: Eq>(candidates: &mut Vec<T>, limit: usize, bucket: impl Fn(&T) -> K) {
    if candidates.len() <= limit {
        return;
    }
    let buckets: Vec<_> = candidates.iter().map(bucket).collect();
    let mut keep = vec![false; candidates.len()];
    let mut remaining = limit;
    while remaining > 0 {
        let mut seen = Vec::new();
        for (index, bucket) in buckets.iter().enumerate() {
            if keep[index] || seen.contains(&bucket) {
                continue;
            }
            keep[index] = true;
            seen.push(bucket);
            remaining -= 1;
            if remaining == 0 {
                break;
            }
        }
    }
    let mut index = 0;
    candidates.retain(|_| {
        let selected = keep[index];
        index += 1;
        selected
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_preserves_family_and_transport_diversity() {
        let addresses = [
            "/ip4/192.168.1.1/tcp/4001",
            "/ip4/192.168.1.2/tcp/4001",
            "/ip4/192.168.1.1/udp/4001/quic-v1",
            "/ip6/::1/tcp/4001",
            "/ip6/::1/udp/4001/quic-v1",
            "/dns/example.com/udp/4001/quic-v1",
            "/dns4/example.com/udp/4001/quic-v1",
            "/dns6/example.com/udp/4001/quic-v1",
            "/udp/4001",
            "/ip4/192.168.1.1",
            "/tcp/4001",
        ]
        .map(|address| address.parse::<Multiaddr>().unwrap());
        let mut candidates = addresses.to_vec();
        retain_diverse(&mut candidates, 8, address_bucket);
        // DNS variants share a bucket. Unknown family/transport remain distinct,
        // and neither duplicate IPv4/TCP nor DNS consumes another bucket's slot.
        let expected = [0, 2, 3, 4, 5, 8, 9, 10].map(|index| addresses[index].clone());
        assert_eq!(candidates, expected);
    }

    #[test]
    fn balances_buckets_without_wasting_spare_capacity() {
        let mut candidates = vec![(0, 1), (1, 1), (2, 1), (3, 1), (4, 2), (5, 3)];
        retain_diverse(&mut candidates, 4, |entry| entry.1);
        assert_eq!(candidates, vec![(0, 1), (1, 1), (4, 2), (5, 3)]);
        let mut one_bucket = (0..20).collect::<Vec<_>>();
        retain_diverse(&mut one_bucket, 8, |_| 1);
        assert_eq!(one_bucket, (0..8).collect::<Vec<_>>());
        retain_diverse(&mut one_bucket, 0, |_| 1);
        assert!(one_bucket.is_empty());
    }
}
