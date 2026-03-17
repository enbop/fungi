use fungi_config::FungiConfig;
use fungi_daemon::FungiDaemon;
use fungi_util::protocols::service_port_protocol;
use libp2p::identity::Keypair;
use std::path::Path;
use std::sync::atomic::{AtomicU16, Ordering};
use tempfile::TempDir;

// Use atomic counter to ensure unique ports for each test
static PORT_COUNTER: AtomicU16 = AtomicU16::new(50020);

fn get_unique_port() -> u16 {
    PORT_COUNTER.fetch_add(10, Ordering::SeqCst)
}

async fn start_test_daemon_in_dir(
    fungi_dir: &Path,
    keypair: Keypair,
    base_port: u16,
) -> FungiDaemon {
    let mut config = FungiConfig::apply_from_dir(fungi_dir).unwrap();
    config.network.listen_tcp_port = base_port;
    config.network.listen_udp_port = base_port + 1000;

    config.file_transfer.server.enabled = false;
    config.file_transfer.proxy_ftp.enabled = false;
    config.file_transfer.proxy_webdav.enabled = false;
    config.save_to_file().unwrap();

    FungiDaemon::start_with(Default::default(), config, keypair, Default::default())
        .await
        .expect("Failed to start test daemon")
}

async fn create_test_daemon() -> (FungiDaemon, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let daemon = start_test_daemon_in_dir(
        temp_dir.path(),
        Keypair::generate_secp256k1(),
        get_unique_port(),
    )
    .await;

    (daemon, temp_dir)
}

async fn create_daemon_pair() -> (FungiDaemon, FungiDaemon, TempDir, TempDir) {
    let client_key = Keypair::generate_secp256k1();
    let server_key = Keypair::generate_secp256k1();

    let client_peer_id = client_key.public().to_peer_id();
    let _server_peer_id = server_key.public().to_peer_id();

    let client_temp_dir = TempDir::new().unwrap();
    let server_temp_dir = TempDir::new().unwrap();

    let mut server_config = FungiConfig::apply_from_dir(server_temp_dir.path()).unwrap();
    server_config
        .network
        .incoming_allowed_peers
        .push(client_peer_id);
    let server_port = get_unique_port();
    server_config.network.listen_tcp_port = server_port;
    server_config.network.listen_udp_port = server_port + 1000;

    server_config.file_transfer.server.enabled = false;
    server_config.file_transfer.proxy_ftp.enabled = false;
    server_config.file_transfer.proxy_webdav.enabled = false;
    server_config.save_to_file().unwrap();

    let mut client_config = FungiConfig::apply_from_dir(client_temp_dir.path()).unwrap();
    let client_port = get_unique_port();
    client_config.network.listen_tcp_port = client_port;
    client_config.network.listen_udp_port = client_port + 1000;

    client_config.file_transfer.server.enabled = false;
    client_config.file_transfer.proxy_ftp.enabled = false;
    client_config.file_transfer.proxy_webdav.enabled = false;
    client_config.save_to_file().unwrap();

    let client_daemon = FungiDaemon::start_with(
        Default::default(),
        client_config,
        client_key,
        Default::default(),
    )
    .await
    .expect("Failed to start client daemon");

    let server_daemon = FungiDaemon::start_with(
        Default::default(),
        server_config,
        server_key,
        Default::default(),
    )
    .await
    .expect("Failed to start server daemon");

    // Connect client to server
    let server_peer_id = server_daemon.swarm_control().local_peer_id();
    let server_addr = format!("/ip4/127.0.0.1/tcp/{server_port}/p2p/{server_peer_id}");

    client_daemon
        .swarm_control()
        .invoke_swarm(move |swarm| {
            swarm.add_peer_address(server_peer_id, server_addr.parse().unwrap())
        })
        .await
        .expect("Failed to add server address to client");

    (
        client_daemon,
        server_daemon,
        client_temp_dir,
        server_temp_dir,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tcp_tunneling_control_creation() {
        let (daemon, _temp_dir) = create_test_daemon().await;

        let tcp_control = daemon.tcp_tunneling_control();

        // Initially should have no rules
        assert_eq!(tcp_control.get_forwarding_rules().len(), 0);
        assert_eq!(tcp_control.get_listening_rules().len(), 0);
    }

    #[tokio::test]
    async fn test_add_forwarding_rule_via_daemon() {
        let (daemon, _temp_dir) = create_test_daemon().await;

        // Generate a valid peer ID for testing
        let test_keypair = Keypair::generate_secp256k1();
        let test_peer_id = test_keypair.public().to_peer_id();

        let rule_id = daemon
            .add_tcp_forwarding_rule(
                "127.0.0.1".to_string(),
                8080,
                test_peer_id.to_string(),
                8080,
            )
            .await
            .unwrap();

        assert!(!rule_id.is_empty());

        // Should now have one rule
        let rules = daemon.get_tcp_forwarding_rules();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].0, rule_id);
        assert_eq!(rules[0].1.local_port, 8080);
    }

    #[tokio::test]
    async fn test_add_listening_rule_via_daemon() {
        let (daemon, _temp_dir) = create_test_daemon().await;

        // Generate a valid peer ID for testing
        let test_keypair = Keypair::generate_secp256k1();
        let test_peer_id = test_keypair.public().to_peer_id();

        let rule_id = daemon
            .add_tcp_listening_rule(
                "127.0.0.1".to_string(),
                9090,
                vec![test_peer_id.to_string()],
            )
            .await
            .unwrap();

        assert!(!rule_id.is_empty());

        // Should now have one rule
        let rules = daemon.get_tcp_listening_rules();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].0, rule_id);
        assert_eq!(rules[0].1.port, 9090);
    }

    #[tokio::test]
    async fn test_remove_rules_via_daemon() {
        let (daemon, _temp_dir) = create_test_daemon().await;

        // Generate valid peer IDs for testing
        let test_keypair1 = Keypair::generate_secp256k1();
        let test_peer_id1 = test_keypair1.public().to_peer_id();
        let test_keypair2 = Keypair::generate_secp256k1();
        let test_peer_id2 = test_keypair2.public().to_peer_id();

        // Add a forwarding rule
        let _forward_rule_id = daemon
            .add_tcp_forwarding_rule(
                "127.0.0.1".to_string(),
                8080,
                test_peer_id1.to_string(),
                8080,
            )
            .await
            .unwrap();

        // Add a listening rule
        let _listen_rule_id = daemon
            .add_tcp_listening_rule(
                "127.0.0.1".to_string(),
                9090,
                vec![test_peer_id2.to_string()],
            )
            .await
            .unwrap();

        // Should have both rules
        assert_eq!(daemon.get_tcp_forwarding_rules().len(), 1);
        assert_eq!(daemon.get_tcp_listening_rules().len(), 1);

        // Remove forwarding rule by its parameters
        let result = daemon.remove_tcp_forwarding_rule(
            "127.0.0.1".to_string(),
            8080,
            test_peer_id1.to_string(),
            8080,
        );
        assert!(result.is_ok());
        assert_eq!(daemon.get_tcp_forwarding_rules().len(), 0);

        // Remove listening rule by its parameters
        let result = daemon.remove_tcp_listening_rule("127.0.0.1".to_string(), 9090);
        assert!(result.is_ok());
        assert_eq!(daemon.get_tcp_listening_rules().len(), 0);
    }

    #[tokio::test]
    async fn test_tcp_tunneling_config_persistence() {
        let (daemon, _temp_dir) = create_test_daemon().await;

        // Generate a valid peer ID for testing
        let test_keypair = Keypair::generate_secp256k1();
        let test_peer_id = test_keypair.public().to_peer_id();

        // Add a rule
        let _rule_id = daemon
            .add_tcp_forwarding_rule(
                "127.0.0.1".to_string(),
                8080,
                test_peer_id.to_string(),
                8080,
            )
            .await
            .unwrap();

        // Check that config was updated
        let config = daemon.get_tcp_tunneling_config();
        assert!(config.forwarding.enabled);
        assert_eq!(config.forwarding.rules.len(), 1);
        assert_eq!(config.forwarding.rules[0].local_port, 8080);
    }

    #[tokio::test]
    async fn test_multiple_forwarding_rules() {
        let (daemon, _temp_dir) = create_test_daemon().await;

        // Generate valid peer IDs for testing
        let test_keypair1 = Keypair::generate_secp256k1();
        let test_peer_id1 = test_keypair1.public().to_peer_id();
        let test_keypair2 = Keypair::generate_secp256k1();
        let test_peer_id2 = test_keypair2.public().to_peer_id();

        // Add multiple forwarding rules
        let rule_id1 = daemon
            .add_tcp_forwarding_rule(
                "127.0.0.1".to_string(),
                8080,
                test_peer_id1.to_string(),
                8888,
            )
            .await
            .unwrap();

        let rule_id2 = daemon
            .add_tcp_forwarding_rule(
                "127.0.0.1".to_string(),
                8081,
                test_peer_id2.to_string(),
                8889,
            )
            .await
            .unwrap();

        // Should have both rules
        let rules = daemon.get_tcp_forwarding_rules();
        assert_eq!(rules.len(), 2);

        // Check rule IDs are different
        assert_ne!(rule_id1, rule_id2);

        // Check config was updated
        let config = daemon.get_tcp_tunneling_config();
        assert_eq!(config.forwarding.rules.len(), 2);
    }

    #[tokio::test]
    async fn test_forwarding_rule_with_details_survives_restart() {
        let temp_dir = TempDir::new().unwrap();

        let daemon = start_test_daemon_in_dir(
            temp_dir.path(),
            Keypair::generate_secp256k1(),
            get_unique_port(),
        )
        .await;

        let remote_peer = Keypair::generate_secp256k1().public().to_peer_id();
        daemon
            .add_tcp_forwarding_rule_with_details(
                "127.0.0.1".to_string(),
                18080,
                remote_peer.to_string(),
                0,
                Some(service_port_protocol("svc.echo", "http")),
                Some("svc.echo".to_string()),
                Some("echo-service".to_string()),
                Some("http".to_string()),
            )
            .await
            .unwrap();

        let mut restarted_config = FungiConfig::apply_from_dir(temp_dir.path()).unwrap();
        let restarted_port = get_unique_port();
        restarted_config.network.listen_tcp_port = restarted_port;
        restarted_config.network.listen_udp_port = restarted_port + 1000;
        restarted_config.file_transfer.server.enabled = false;
        restarted_config.file_transfer.proxy_ftp.enabled = false;
        restarted_config.file_transfer.proxy_webdav.enabled = false;
        restarted_config.save_to_file().unwrap();

        let restarted = FungiDaemon::start_with(
            Default::default(),
            restarted_config,
            Keypair::generate_secp256k1(),
            Default::default(),
        )
        .await
        .expect("Failed to restart test daemon");

        let rules = restarted.get_tcp_forwarding_rules();
        assert_eq!(rules.len(), 1);
        let rule = &rules[0].1;
        assert_eq!(rule.local_host, "127.0.0.1");
        assert_eq!(rule.local_port, 18080);
        assert_eq!(rule.remote_peer_id, remote_peer.to_string());
        assert_eq!(
            rule.remote_protocol.as_deref(),
            Some(service_port_protocol("svc.echo", "http").as_str())
        );
        assert_eq!(rule.remote_service_id.as_deref(), Some("svc.echo"));
        assert_eq!(rule.remote_service_name.as_deref(), Some("echo-service"));
        assert_eq!(rule.remote_service_port_name.as_deref(), Some("http"));
    }

    #[tokio::test]
    async fn test_daemon_pair_connection() {
        let (client_daemon, server_daemon, _client_temp, _server_temp) = create_daemon_pair().await;

        // Verify daemons can see each other
        let client_peer_id = client_daemon.swarm_control().local_peer_id();
        let server_peer_id = server_daemon.swarm_control().local_peer_id();

        assert_ne!(client_peer_id, server_peer_id);

        // Basic connectivity test - each daemon should exist
        assert_eq!(client_daemon.get_tcp_forwarding_rules().len(), 0);
        assert_eq!(server_daemon.get_tcp_forwarding_rules().len(), 0);
    }
}
