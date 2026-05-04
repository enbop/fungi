//! Smoke tests for fungi's explicit probe ping path.

use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use fungi_daemon::test_support::{TestDaemon, TestDaemonBuilder, spawn_connected_pair};
use libp2p::identity::Keypair;
use libp2p::{PeerId, swarm::ConnectionId, swarm::dial_opts::DialOpts};

async fn wait_for_outbound_connection(
    daemon: &TestDaemon,
    peer_id: PeerId,
    timeout: Duration,
) -> Result<ConnectionId> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(connections) = daemon.daemon().get_peer_connections(peer_id)
            && let Some(connection) = connections.iter().find(|connection| {
                matches!(
                    connection.direction,
                    fungi_swarm::ConnectionDirection::Outbound
                )
            })
        {
            return Ok(connection.connection_id());
        }

        if Instant::now() >= deadline {
            return Err(anyhow!(
                "timed out ({timeout:?}) waiting for outbound connection to {peer_id}"
            ));
        }

        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn active_probe_ping_returns_rtt_and_updates_connection_state() -> Result<()> {
    let (client, server) = spawn_connected_pair().await?;

    let server_peer_id = server.peer_id();
    let server_addr = server.tcp_multiaddr();
    client
        .swarm_control()
        .invoke_swarm(move |swarm| {
            swarm.dial(
                DialOpts::peer_id(server_peer_id)
                    .addresses(vec![server_addr])
                    .build(),
            )
        })
        .await??;
    client
        .wait_connected(server.peer_id(), Duration::from_secs(5))
        .await?;
    server
        .wait_connected(client.peer_id(), Duration::from_secs(5))
        .await?;

    let connection_id =
        wait_for_outbound_connection(&client, server.peer_id(), Duration::from_secs(5)).await?;

    let rtt = client
        .daemon()
        .swarm_control()
        .ping_connection(connection_id, Duration::from_secs(2))
        .await?;

    assert!(
        rtt < Duration::from_secs(2),
        "probe RTT should complete before timeout, got {rtt:?}"
    );

    let connection_id = connection_id.to_string();
    let snapshot = client
        .daemon()
        .list_connections(Some(server.peer_id()))
        .into_iter()
        .find(|snapshot| {
            snapshot.direction == "outbound" && snapshot.connection_id == connection_id
        })
        .ok_or_else(|| anyhow!("missing outbound connection snapshot {connection_id}"))?;

    assert!(
        snapshot.last_ping_at.is_some(),
        "active probe should update connection ping metadata"
    );

    Ok(())
}

async fn wait_for_inbound_connection(
    daemon: &TestDaemon,
    peer_id: PeerId,
    timeout: Duration,
) -> Result<ConnectionId> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(connections) = daemon.daemon().get_peer_connections(peer_id)
            && let Some(connection) = connections.iter().find(|connection| {
                matches!(
                    connection.direction,
                    fungi_swarm::ConnectionDirection::Inbound
                )
            })
        {
            return Ok(connection.connection_id());
        }

        if Instant::now() >= deadline {
            return Err(anyhow!(
                "timed out ({timeout:?}) waiting for inbound connection from {peer_id}"
            ));
        }

        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn active_probe_ping_can_use_inbound_connection_id() -> Result<()> {
    let server_kp = Keypair::generate_ed25519();
    let server_peer_id = server_kp.public().to_peer_id();
    let client_kp = Keypair::generate_ed25519();
    let client_peer_id = client_kp.public().to_peer_id();

    let server = TestDaemonBuilder::new()
        .with_keypair(server_kp)
        .with_trusted_device(client_peer_id)
        .build()
        .await?;
    let client = TestDaemonBuilder::new()
        .with_keypair(client_kp)
        .with_trusted_device(server_peer_id)
        .build()
        .await?;

    let client_addr = client.tcp_multiaddr();
    server
        .swarm_control()
        .invoke_swarm(move |swarm| {
            swarm.dial(
                DialOpts::peer_id(client_peer_id)
                    .addresses(vec![client_addr])
                    .build(),
            )
        })
        .await??;
    server
        .wait_connected(client.peer_id(), Duration::from_secs(5))
        .await?;
    client
        .wait_connected(server.peer_id(), Duration::from_secs(5))
        .await?;

    let connection_id =
        wait_for_inbound_connection(&client, server.peer_id(), Duration::from_secs(5)).await?;

    let rtt = client
        .daemon()
        .swarm_control()
        .ping_connection(connection_id, Duration::from_secs(2))
        .await?;

    assert!(
        rtt < Duration::from_secs(2),
        "probe RTT over inbound connection should complete before timeout, got {rtt:?}"
    );

    let connection_id = connection_id.to_string();
    let snapshot = client
        .daemon()
        .list_connections(Some(server.peer_id()))
        .into_iter()
        .find(|snapshot| snapshot.direction == "inbound" && snapshot.connection_id == connection_id)
        .ok_or_else(|| anyhow!("missing inbound connection snapshot {connection_id}"))?;

    assert!(
        snapshot.last_ping_at.is_some(),
        "active probe should update ping metadata for inbound connections too"
    );

    Ok(())
}
