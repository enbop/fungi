//! Integration tests for TCP tunneling via [`FungiDaemon`].
//!
//! All daemon instances are created via [`fungi_daemon::test_support`] so that helper
//! logic lives in one place and tests stay readable.

use fungi_config::FungiConfig;
use fungi_daemon::FungiDaemon;
use fungi_daemon::test_support::{
    TestDaemon, TestDaemonBuilder, reserve_ephemeral_port, spawn_connected_pair,
};
use fungi_util::protocols::service_port_protocol;
use libp2p::identity::Keypair;
use tempfile::TempDir;

// ── Helpers needed only for the "restart" test (needs a persistent dir) ───────

async fn spawn_daemon_in_dir(dir: &TempDir) -> FungiDaemon {
    let port = reserve_ephemeral_port();
    let mut config = FungiConfig::apply_from_dir(dir.path()).unwrap();
    config.network.listen_tcp_port = port;
    config.network.listen_udp_port = port.wrapping_add(1000);
    config.network.relay_enabled = false;
    config.file_transfer.proxy_ftp.enabled = false;
    config.file_transfer.proxy_webdav.enabled = false;
    config.save_to_file().unwrap();
    FungiDaemon::start_with(
        Default::default(),
        config,
        Keypair::generate_ed25519(),
        Default::default(),
    )
    .await
    .expect("failed to start daemon")
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn tunneling_control_starts_with_no_rules() {
    let d = TestDaemon::spawn().await.unwrap();
    let ctrl = d.daemon().tcp_tunneling_control();
    assert_eq!(ctrl.get_forwarding_rules().len(), 0);
    assert_eq!(ctrl.get_listening_rules().len(), 0);
}

#[tokio::test]
async fn add_forwarding_rule_returns_nonempty_id() {
    let d = TestDaemon::spawn().await.unwrap();
    let remote = Keypair::generate_ed25519().public().to_peer_id();

    let rule_id = d
        .daemon()
        .add_tcp_forwarding_rule("127.0.0.1".into(), 8080, remote.to_string(), 8080)
        .await
        .unwrap();

    assert!(!rule_id.is_empty());
    let rules = d.daemon().get_tcp_forwarding_rules();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].0, rule_id);
    assert_eq!(rules[0].1.local_port, 8080);
}

#[tokio::test]
async fn add_listening_rule_returns_nonempty_id() {
    let d = TestDaemon::spawn().await.unwrap();
    let remote = Keypair::generate_ed25519().public().to_peer_id();

    let rule_id = d
        .daemon()
        .add_tcp_listening_rule("127.0.0.1".into(), 9090, vec![remote.to_string()])
        .await
        .unwrap();

    assert!(!rule_id.is_empty());
    let rules = d.daemon().get_tcp_listening_rules();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].0, rule_id);
    assert_eq!(rules[0].1.port, 9090);
}

#[tokio::test]
async fn remove_forwarding_and_listening_rules() {
    let d = TestDaemon::spawn().await.unwrap();
    let p1 = Keypair::generate_ed25519().public().to_peer_id();
    let p2 = Keypair::generate_ed25519().public().to_peer_id();

    d.daemon()
        .add_tcp_forwarding_rule("127.0.0.1".into(), 8080, p1.to_string(), 8080)
        .await
        .unwrap();
    d.daemon()
        .add_tcp_listening_rule("127.0.0.1".into(), 9090, vec![p2.to_string()])
        .await
        .unwrap();

    assert_eq!(d.daemon().get_tcp_forwarding_rules().len(), 1);
    assert_eq!(d.daemon().get_tcp_listening_rules().len(), 1);

    d.daemon()
        .remove_tcp_forwarding_rule("127.0.0.1".into(), 8080, p1.to_string(), 8080)
        .unwrap();
    assert_eq!(d.daemon().get_tcp_forwarding_rules().len(), 0);

    d.daemon()
        .remove_tcp_listening_rule("127.0.0.1".into(), 9090)
        .unwrap();
    assert_eq!(d.daemon().get_tcp_listening_rules().len(), 0);
}

#[tokio::test]
async fn added_forwarding_rule_is_reflected_in_config() {
    let d = TestDaemon::spawn().await.unwrap();
    let remote = Keypair::generate_ed25519().public().to_peer_id();

    d.daemon()
        .add_tcp_forwarding_rule("127.0.0.1".into(), 8080, remote.to_string(), 8080)
        .await
        .unwrap();

    let cfg = d.daemon().get_tcp_tunneling_config();
    assert!(cfg.forwarding.enabled);
    assert_eq!(cfg.forwarding.rules.len(), 1);
    assert_eq!(cfg.forwarding.rules[0].local_port, 8080);
}

#[tokio::test]
async fn multiple_forwarding_rules_have_distinct_ids() {
    let d = TestDaemon::spawn().await.unwrap();
    let p1 = Keypair::generate_ed25519().public().to_peer_id();
    let p2 = Keypair::generate_ed25519().public().to_peer_id();

    let id1 = d
        .daemon()
        .add_tcp_forwarding_rule("127.0.0.1".into(), 8080, p1.to_string(), 8888)
        .await
        .unwrap();
    let id2 = d
        .daemon()
        .add_tcp_forwarding_rule("127.0.0.1".into(), 8081, p2.to_string(), 8889)
        .await
        .unwrap();

    assert_ne!(id1, id2);
    assert_eq!(d.daemon().get_tcp_forwarding_rules().len(), 2);
    assert_eq!(
        d.daemon().get_tcp_tunneling_config().forwarding.rules.len(),
        2
    );
}

#[tokio::test]
async fn forwarding_rule_with_details_survives_daemon_restart() {
    let dir = TempDir::new().unwrap();
    let daemon = spawn_daemon_in_dir(&dir).await;

    let remote = Keypair::generate_ed25519().public().to_peer_id();
    daemon
        .add_tcp_forwarding_rule_with_details(
            "127.0.0.1".into(),
            18080,
            remote.to_string(),
            0,
            Some(service_port_protocol("svc.echo", "http")),
            Some("svc.echo".into()),
            Some("echo-service".into()),
            Some("http".into()),
        )
        .await
        .unwrap();

    // Simulate restart by starting a new daemon in the same dir.
    let restarted = spawn_daemon_in_dir(&dir).await;
    let rules = restarted.get_tcp_forwarding_rules();
    assert_eq!(rules.len(), 1);
    let rule = &rules[0].1;
    assert_eq!(rule.local_host, "127.0.0.1");
    assert_eq!(rule.local_port, 18080);
    assert_eq!(rule.remote_peer_id, remote.to_string());
    assert_eq!(
        rule.remote_protocol.as_deref(),
        Some(service_port_protocol("svc.echo", "http").as_str())
    );
    assert_eq!(rule.remote_service_id.as_deref(), Some("svc.echo"));
    assert_eq!(rule.remote_service_name.as_deref(), Some("echo-service"));
    assert_eq!(rule.remote_service_port_name.as_deref(), Some("http"));
}

#[tokio::test]
async fn connected_pair_has_distinct_peer_ids_and_empty_rules() {
    let (client, server) = spawn_connected_pair().await.unwrap();

    assert_ne!(client.peer_id(), server.peer_id());
    assert_eq!(client.daemon().get_tcp_forwarding_rules().len(), 0);
    assert_eq!(server.daemon().get_tcp_forwarding_rules().len(), 0);
}

#[tokio::test]
async fn builder_sets_allowed_peer_in_config() {
    let allowed = Keypair::generate_ed25519().public().to_peer_id();
    let d = TestDaemonBuilder::new()
        .with_allowed_peer(allowed)
        .build()
        .await
        .unwrap();

    let has_peer = d
        .daemon()
        .config()
        .lock()
        .network
        .incoming_allowed_peers
        .contains(&allowed);
    assert!(has_peer);
}
