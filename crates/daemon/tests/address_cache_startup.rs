use fungi_config::direct_addresses::DirectAddressCache;
use fungi_daemon::test_support::TestDaemonBuilder;
use fungi_util::address_policy::DIRECT_ADDRESS_RETENTION;
use std::time::{Duration, SystemTime};

#[tokio::test]
async fn startup_manager_persists_cleanup_without_any_new_connections() {
    let daemon = TestDaemonBuilder::new()
        .with_config(|config| {
            let dir = config.config_file_path().parent().unwrap();
            let mut cache = DirectAddressCache::apply_from_dir(dir).unwrap();
            cache.record_successes(
                libp2p::PeerId::random().to_string(),
                vec!["/ip4/192.168.1.145/tcp/4001".into()],
                SystemTime::now() - DIRECT_ADDRESS_RETENTION - Duration::from_secs(86400),
            );
            cache.save_to_file().unwrap();
        })
        .build()
        .await
        .unwrap();

    let file = daemon.fungi_dir().join("cache/direct_addresses.json");
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            // Inspect raw persistence: loading via apply_from_dir would prune
            // the fixture again and could conceal a missing manager handoff.
            let disk: DirectAddressCache =
                serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
            if disk.devices.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the initial manager tick must persist startup cleanup even without connections");
}
