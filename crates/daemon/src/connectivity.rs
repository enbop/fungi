use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::{Duration, SystemTime},
};

use fungi_config::{devices::DeviceInfo, direct_addresses::DirectAddressCache};
use fungi_swarm::{ConnectionDirection, PeerAddressSource, State, SwarmControl};
use libp2p::Multiaddr;
use parking_lot::Mutex;
use tokio::task::JoinHandle;

use crate::controls::mdns::MdnsControl;

const DIRECT_ADDRESS_CACHE_MAINTENANCE_INTERVAL: Duration = Duration::from_secs(30);

struct ConnectivityInner {
    swarm: SwarmControl,
    mdns: MdnsControl,
    direct_address_cache: Arc<Mutex<DirectAddressCache>>,
}

/// Shared connectivity capabilities backed by the daemon's single libp2p swarm.
///
/// Domain handles share this boundary instead of carrying independent low-level network controls.
#[derive(Clone)]
pub struct Connectivity {
    inner: Arc<ConnectivityInner>,
}

impl Connectivity {
    pub(crate) fn new(
        swarm: SwarmControl,
        mdns: MdnsControl,
        direct_address_cache: DirectAddressCache,
    ) -> Self {
        Self {
            inner: Arc::new(ConnectivityInner {
                swarm,
                mdns,
                direct_address_cache: Arc::new(Mutex::new(direct_address_cache)),
            }),
        }
    }

    pub fn swarm_control(&self) -> &SwarmControl {
        &self.inner.swarm
    }

    pub fn mdns_control(&self) -> &MdnsControl {
        &self.inner.mdns
    }

    pub(crate) fn record_device_addresses(&self, device_info: &DeviceInfo) {
        for address in &device_info.multiaddrs {
            match address.parse::<Multiaddr>() {
                Ok(multiaddr) => {
                    self.inner.swarm.state().record_peer_address(
                        device_info.peer_id,
                        multiaddr,
                        PeerAddressSource::DeviceConfig,
                    );
                }
                Err(error) => log::debug!(
                    "Ignoring invalid device multiaddr for peer {}: {} ({})",
                    device_info.peer_id,
                    address,
                    error
                ),
            }
        }
    }

    pub(crate) fn spawn_direct_address_cache_manager(&self) -> JoinHandle<()> {
        let swarm = self.inner.swarm.clone();
        let direct_address_cache = self.inner.direct_address_cache.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(DIRECT_ADDRESS_CACHE_MAINTENANCE_INTERVAL);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            let mut last_synced_pairs = BTreeSet::<(String, String)>::new();

            let mut dirty = direct_address_cache.lock().take_pending_persistence();
            loop {
                interval.tick().await;

                let active = collect_direct_connection_addresses(swarm.state());
                {
                    let mut cache = direct_address_cache.lock();
                    let changed = maintain_direct_address_cache(
                        &mut cache,
                        active,
                        &mut last_synced_pairs,
                        SystemTime::now(),
                    );
                    if changed {
                        hydrate_direct_address_cache(swarm.state(), &cache);
                    }
                    dirty |= changed;
                }
                if !dirty {
                    continue;
                }

                // This task is the only writer. Never hold the cache lock across
                // file I/O, and keep retrying the latest snapshot after a failure.
                let snapshot = direct_address_cache.lock().clone();
                if let Err(error) = persist_direct_address_cache(&snapshot, &mut dirty) {
                    log::warn!("Failed to save direct address cache; will retry: {error}");
                }
            }
        })
    }
}

fn persist_direct_address_cache(
    cache: &DirectAddressCache,
    dirty: &mut bool,
) -> anyhow::Result<()> {
    if *dirty {
        cache.save_to_file()?;
        *dirty = false;
    }
    Ok(())
}

fn maintain_direct_address_cache(
    cache: &mut DirectAddressCache,
    active: BTreeMap<String, Vec<String>>,
    last_synced_pairs: &mut BTreeSet<(String, String)>,
    now: SystemTime,
) -> bool {
    let new_successes = new_direct_address_successes(&active, last_synced_pairs);
    let mut changed = false;
    for (peer_id, addresses) in new_successes {
        let recorded = cache.record_successes(peer_id, addresses, now);
        changed |= recorded;
    }
    for (peer_id, addresses) in active {
        let refreshed = cache.refresh_active_addresses(&peer_id, &addresses, now);
        changed |= refreshed;
    }
    // Cleanup must run even when every peer is offline and there are no successes.
    let pruned = cache.prune(now);
    changed || pruned
}

pub(crate) fn hydrate_direct_address_cache(state: &State, cache: &DirectAddressCache) {
    let mut loaded = 0usize;
    let mut ignored = 0usize;

    for device in &cache.devices {
        let Ok(peer_id) = device.peer_id.parse::<libp2p::PeerId>() else {
            ignored += device.addresses.len();
            continue;
        };

        for entry in &device.addresses {
            match entry.address.parse::<Multiaddr>() {
                Ok(multiaddr) => {
                    match state.restore_peer_address_record(
                        peer_id,
                        multiaddr,
                        PeerAddressSource::DirectCache,
                        entry.first_success_at,
                        entry.last_success_at,
                        entry.success_count,
                    ) {
                        fungi_swarm::PeerAddressObservation::New
                        | fungi_swarm::PeerAddressObservation::Refreshed => loaded += 1,
                        fungi_swarm::PeerAddressObservation::Ignored => ignored += 1,
                    }
                }
                Err(error) => {
                    ignored += 1;
                    log::debug!(
                        "Ignoring invalid cached direct address for peer {}: {} ({})",
                        device.peer_id,
                        entry.address,
                        error
                    );
                }
            }
        }
    }

    if loaded > 0 || ignored > 0 {
        log::info!(
            "Loaded {} cached direct address(es) into dial planner state (ignored={})",
            loaded,
            ignored
        );
    }
}

fn collect_direct_connection_addresses(state: &State) -> BTreeMap<String, Vec<String>> {
    let mut grouped = BTreeMap::<String, Vec<String>>::new();
    for peer_id in state.connected_peer_ids() {
        for connection in state.get_connections_by_peer_id(&peer_id) {
            if !matches!(connection.direction, ConnectionDirection::Outbound)
                || connection.is_relay()
            {
                continue;
            }

            grouped
                .entry(peer_id.to_string())
                .or_default()
                .push(connection.remote_addr.to_string());
        }
    }

    normalize_direct_address_groups(grouped)
}

fn new_direct_address_successes(
    grouped: &BTreeMap<String, Vec<String>>,
    last_synced_pairs: &mut BTreeSet<(String, String)>,
) -> BTreeMap<String, Vec<String>> {
    let current_pairs = direct_address_pairs(grouped);
    let mut new_pairs = BTreeMap::<String, Vec<String>>::new();

    for (peer_id, address) in current_pairs.difference(last_synced_pairs) {
        new_pairs
            .entry(peer_id.clone())
            .or_default()
            .push(address.clone());
    }

    *last_synced_pairs = current_pairs;
    new_pairs
}

fn direct_address_pairs(grouped: &BTreeMap<String, Vec<String>>) -> BTreeSet<(String, String)> {
    grouped
        .iter()
        .flat_map(|(peer_id, addresses)| {
            addresses
                .iter()
                .map(|address| (peer_id.clone(), address.clone()))
        })
        .collect()
}

fn normalize_direct_address_groups(
    mut grouped: BTreeMap<String, Vec<String>>,
) -> BTreeMap<String, Vec<String>> {
    grouped.retain(|_, addresses| {
        addresses.retain(|address| !address.trim().is_empty());
        for address in addresses.iter_mut() {
            *address = address.trim().to_string();
        }
        addresses.sort();
        addresses.dedup();
        !addresses.is_empty()
    });
    grouped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_direct_address_successes_only_returns_new_pairs() {
        let mut last_synced_pairs = BTreeSet::new();

        let first = new_direct_address_successes(
            &BTreeMap::from([
                (
                    "peer-a".to_string(),
                    vec!["/ip4/192.168.1.7/tcp/4001".to_string()],
                ),
                (
                    "peer-b".to_string(),
                    vec!["/ip4/192.168.1.8/tcp/4001".to_string()],
                ),
            ]),
            &mut last_synced_pairs,
        );
        assert_eq!(first.len(), 2);

        let second = new_direct_address_successes(
            &BTreeMap::from([
                (
                    "peer-a".to_string(),
                    vec!["/ip4/192.168.1.7/tcp/4001".to_string()],
                ),
                (
                    "peer-b".to_string(),
                    vec![
                        "/ip4/192.168.1.8/tcp/4001".to_string(),
                        "/ip4/192.168.1.9/tcp/4001".to_string(),
                    ],
                ),
            ]),
            &mut last_synced_pairs,
        );
        assert_eq!(
            second,
            BTreeMap::from([(
                "peer-b".to_string(),
                vec!["/ip4/192.168.1.9/tcp/4001".to_string()]
            )])
        );

        let third = new_direct_address_successes(
            &BTreeMap::from([
                (
                    "peer-a".to_string(),
                    vec!["/ip4/192.168.1.7/tcp/4001".to_string()],
                ),
                (
                    "peer-b".to_string(),
                    vec![
                        "/ip4/192.168.1.8/tcp/4001".to_string(),
                        "/ip4/192.168.1.9/tcp/4001".to_string(),
                    ],
                ),
            ]),
            &mut last_synced_pairs,
        );
        assert!(third.is_empty());

        let empty = new_direct_address_successes(&BTreeMap::new(), &mut last_synced_pairs);
        assert!(empty.is_empty());

        let after_disconnect = new_direct_address_successes(
            &BTreeMap::from([(
                "peer-a".to_string(),
                vec!["/ip4/192.168.1.7/tcp/4001".to_string()],
            )]),
            &mut last_synced_pairs,
        );
        assert_eq!(
            after_disconnect,
            BTreeMap::from([(
                "peer-a".to_string(),
                vec!["/ip4/192.168.1.7/tcp/4001".to_string()]
            )])
        );
    }
    #[test]
    fn cache_maintenance_cleans_idle_cache_and_refreshes_live_connections() {
        use fungi_util::address_policy::DIRECT_ADDRESS_RETENTION;
        let mut cache = DirectAddressCache::default();
        let now = SystemTime::now();
        let address = "/ip4/192.168.1.145/tcp/4001".to_owned();
        let active = BTreeMap::from([("peer".into(), vec![address.clone()])]);
        let mut synced = BTreeSet::new();
        assert!(maintain_direct_address_cache(
            &mut cache,
            active.clone(),
            &mut synced,
            now
        ));
        assert!(!maintain_direct_address_cache(
            &mut cache,
            active.clone(),
            &mut synced,
            now + Duration::from_secs(30)
        ));
        let later = now + DIRECT_ADDRESS_RETENTION + Duration::from_secs(1);
        assert!(maintain_direct_address_cache(
            &mut cache,
            active,
            &mut synced,
            later
        ));
        assert_eq!(cache.get_device_addresses("peer"), vec![address]);
        assert_eq!(cache.devices[0].addresses[0].success_count, 1);
        let expired = later + DIRECT_ADDRESS_RETENTION + Duration::from_secs(1);
        assert!(maintain_direct_address_cache(
            &mut cache,
            BTreeMap::new(),
            &mut synced,
            expired
        ));
        assert!(cache.devices.is_empty());
        assert!(!maintain_direct_address_cache(
            &mut cache,
            BTreeMap::new(),
            &mut synced,
            expired
        ));
    }
    #[test]
    fn idle_cleanup_is_persisted_and_failed_writes_retry_without_new_successes() {
        use fungi_util::address_policy::DIRECT_ADDRESS_RETENTION;
        let dir = tempfile::tempdir().unwrap();
        let mut cache = DirectAddressCache::apply_from_dir(dir.path()).unwrap();
        let now = SystemTime::now();
        cache.record_successes("peer".into(), vec!["/ip4/192.168.1.9/tcp/4001".into()], now);
        cache.save_to_file().unwrap();
        let mut dirty = maintain_direct_address_cache(
            &mut cache,
            BTreeMap::new(),
            &mut BTreeSet::new(),
            now + DIRECT_ADDRESS_RETENTION + Duration::from_secs(1),
        );
        assert!(dirty);

        // A directory in place of the destination causes atomic replacement to fail.
        let file = dir.path().join("cache/direct_addresses.json");
        std::fs::remove_file(&file).unwrap();
        std::fs::create_dir(&file).unwrap();
        assert!(persist_direct_address_cache(&cache, &mut dirty).is_err());
        assert!(dirty);
        std::fs::remove_dir(&file).unwrap();
        persist_direct_address_cache(&cache, &mut dirty).unwrap();
        assert!(!dirty);
        let disk = DirectAddressCache::apply_from_dir(dir.path()).unwrap();
        assert!(disk.devices.is_empty());
    }

    #[test]
    fn startup_cleanup_is_retried_on_idle_ticks_until_persistence_succeeds() {
        use fungi_util::address_policy::DIRECT_ADDRESS_RETENTION;
        let dir = tempfile::tempdir().unwrap();
        let mut cache = DirectAddressCache::apply_from_dir(dir.path()).unwrap();
        let now = SystemTime::now();
        cache.record_successes(
            "peer".into(),
            vec!["/ip4/192.168.1.9/tcp/4001".into()],
            now - DIRECT_ADDRESS_RETENTION - Duration::from_secs(1),
        );
        cache.save_to_file().unwrap();
        let mut loaded = DirectAddressCache::apply_from_dir(dir.path()).unwrap();
        assert!(loaded.devices.is_empty());
        let mut dirty = loaded.take_pending_persistence();
        let mut synced = BTreeSet::new();
        assert!(!maintain_direct_address_cache(
            &mut loaded,
            BTreeMap::new(),
            &mut synced,
            now
        ));
        assert!(dirty); // startup work survives an idle tick with no further pruning

        let file = dir.path().join("cache/direct_addresses.json");
        std::fs::remove_file(&file).unwrap();
        std::fs::create_dir(&file).unwrap();
        assert!(persist_direct_address_cache(&loaded, &mut dirty).is_err());
        assert!(dirty);
        std::fs::remove_dir(&file).unwrap();
        let changed = maintain_direct_address_cache(
            &mut loaded,
            BTreeMap::new(),
            &mut synced,
            now + Duration::from_secs(30),
        );
        dirty |= changed;
        persist_direct_address_cache(&loaded, &mut dirty).unwrap();
        assert!(!dirty);
        let disk: DirectAddressCache =
            serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
        assert!(disk.devices.is_empty());
    }

    #[test]
    fn successful_address_refresh_reaches_memory_without_losing_discovery_source() {
        use fungi_util::address_policy::DIRECT_ADDRESS_RETENTION;
        let peer = libp2p::PeerId::random();
        let address = "/ip4/192.168.1.145/udp/5001/quic-v1";
        let now = SystemTime::now();
        let state = State::default();
        state.restore_peer_address_record(
            peer,
            address.parse().unwrap(),
            PeerAddressSource::Mdns,
            now,
            now,
            1,
        );
        let mut cache = DirectAddressCache::default();
        let later = now + DIRECT_ADDRESS_RETENTION;
        cache.record_successes(peer.to_string(), vec![address.to_owned()], later);
        hydrate_direct_address_cache(&state, &cache);
        let records = state.peer_addresses(&peer);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].last_observed_at, later);
        assert_eq!(records[0].source, PeerAddressSource::Mdns);
    }
}
