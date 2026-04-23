use std::time::Duration;

use anyhow::Result;
use fungi_config::file_transfer::FileTransferClient;
use fungi_daemon::test_support::{TestDaemon, TestDaemonBuilder};
use libp2p::{identity::Keypair, swarm::dial_opts::DialOpts};
use tempfile::TempDir;

async fn spawn_reverse_connected_pair() -> Result<(TestDaemon, TestDaemon)> {
    let victim_kp = Keypair::generate_ed25519();
    let victim_peer_id = victim_kp.public().to_peer_id();
    let attacker_kp = Keypair::generate_ed25519();

    let victim = TestDaemonBuilder::new()
        .with_keypair(victim_kp)
        .build()
        .await?;
    let attacker = TestDaemonBuilder::new()
        .with_keypair(attacker_kp)
        .with_allowed_peer(victim_peer_id)
        .build()
        .await?;

    let attacker_peer_id = attacker.peer_id();
    let attacker_addr = attacker.tcp_multiaddr();
    victim
        .swarm_control()
        .invoke_swarm(move |swarm| {
            swarm.dial(
                DialOpts::peer_id(attacker_peer_id)
                    .addresses(vec![attacker_addr])
                    .build(),
            )
        })
        .await??;
    victim
        .wait_connected(attacker.peer_id(), Duration::from_secs(5))
        .await?;
    attacker
        .wait_connected(victim.peer_id(), Duration::from_secs(5))
        .await?;

    Ok((victim, attacker))
}

fn assert_only_inbound_connection(local: &TestDaemon, remote: &TestDaemon) {
    let connections = local
        .daemon()
        .get_peer_connections(remote.peer_id())
        .expect("expected at least one connection");

    assert!(
        !connections.is_empty(),
        "expected at least one connection to {}",
        remote.peer_id()
    );
    assert!(
        connections.iter().all(|connection| {
            matches!(
                connection.direction,
                fungi_swarm::ConnectionDirection::Inbound
            )
        }),
        "expected only inbound connections from {} to {}",
        remote.peer_id(),
        local.peer_id()
    );
}

#[tokio::test]
async fn disallowed_peer_cannot_use_service_control_over_existing_inbound_connection() -> Result<()>
{
    let (victim, attacker) = spawn_reverse_connected_pair().await?;
    assert_only_inbound_connection(&attacker, &victim);

    let result = attacker
        .daemon()
        .service_control_protocol_control()
        .list_peer_services(victim.peer_id())
        .await;

    assert!(
        result.is_err(),
        "disallowed peer should not be able to use service control over an existing inbound connection"
    );
    Ok(())
}

#[tokio::test]
async fn disallowed_peer_cannot_use_node_capabilities_over_existing_inbound_connection()
-> Result<()> {
    let (victim, attacker) = spawn_reverse_connected_pair().await?;
    assert_only_inbound_connection(&attacker, &victim);

    let result = attacker
        .daemon()
        .node_capabilities_control()
        .discover_peer_capabilities(victim.peer_id())
        .await;

    assert!(
        result.is_err(),
        "disallowed peer should not be able to discover node capabilities over an existing inbound connection"
    );
    Ok(())
}

#[tokio::test]
async fn disallowed_peer_cannot_use_file_transfer_over_existing_inbound_connection() -> Result<()> {
    let (victim, attacker) = spawn_reverse_connected_pair().await?;
    assert_only_inbound_connection(&attacker, &victim);

    let shared_root = TempDir::new()?;
    victim
        .daemon()
        .start_file_transfer_service(shared_root.path().to_string_lossy().to_string())
        .await?;

    attacker
        .daemon()
        .ftc_control()
        .add_client(FileTransferClient {
            enabled: true,
            name: Some("victim".to_string()),
            peer_id: victim.peer_id(),
        });

    let result = attacker.daemon().ftc_control().get_client("victim").await;

    assert!(
        result.is_err(),
        "disallowed peer should not be able to establish a file transfer RPC client over an existing inbound connection"
    );
    Ok(())
}
