use fungi_daemon::test_support::{TestDaemon, spawn_connected_pair};
use std::time::Duration;

// Create client and server daemons for TCP tunneling testing
async fn create_tcp_tunneling_daemons() -> (TestDaemon, TestDaemon) {
    let (client_daemon, server_daemon) = spawn_connected_pair()
        .await
        .expect("Failed to create daemon pair");

    client_daemon
        .connect_to(&server_daemon)
        .await
        .expect("Failed to connect client to server");
    client_daemon
        .wait_connected(server_daemon.peer_id(), Duration::from_secs(5))
        .await
        .expect("Client did not observe the server connection");
    server_daemon
        .wait_connected(client_daemon.peer_id(), Duration::from_secs(5))
        .await
        .expect("Server did not observe the client connection");

    println!(
        "✅ Client daemon started on port {}",
        client_daemon.tcp_port
    );
    println!(
        "✅ Server daemon started on port {}",
        server_daemon.tcp_port
    );
    println!("✅ Client peer ID: {}", client_daemon.peer_id());
    println!("✅ Server peer ID: {}", server_daemon.peer_id());

    (client_daemon, server_daemon)
}

#[tokio::main]
async fn main() {
    env_logger::init();

    println!("🚀 Starting TCP Tunneling Test Program");
    println!("=======================================");

    let (client_daemon, server_daemon) = create_tcp_tunneling_daemons().await;

    let server_peer_id = server_daemon.peer_id();

    println!("🔧 Setting up test tunneling rules...");

    // Add a listening rule on server (port 8888 -> tunneled traffic)
    match server_daemon
        .daemon()
        .add_tcp_listening_rule("127.0.0.1".to_string(), 8888, vec![])
        .await
    {
        Ok(listen_rule_id) => {
            println!("✅ Added listening rule: {listen_rule_id}");
            println!("   Server listening on: 127.0.0.1:8888 (protocol: /fungi/tunnel/0.1.0/8888)");
        }
        Err(e) => println!("❌ Failed to add listening rule: {e}"),
    }

    // Add a forwarding rule on client (port 7777 -> server:8888)
    match client_daemon
        .daemon()
        .add_tcp_forwarding_rule(
            "127.0.0.1".to_string(),
            7777,
            server_peer_id.to_string(),
            8888,
        )
        .await
    {
        Ok(forward_rule_id) => {
            println!("✅ Added forwarding rule: {forward_rule_id}");
            println!("   Client forwarding: 127.0.0.1:7777 -> {server_peer_id} (127.0.0.1:8888)");
        }
        Err(e) => println!("❌ Failed to add forwarding rule: {e}"),
    }

    println!();
    println!("🔗 TCP Tunnel Setup Complete!");
    println!("===============================");
    println!("📋 Test Setup:");
    println!("   Client peer ID: {}", client_daemon.peer_id());
    println!("   Server peer ID: {server_peer_id}");
    println!("   Client TCP port: {}", client_daemon.tcp_port);
    println!("   Server TCP port: {}", server_daemon.tcp_port);
    println!();
    println!("🧪 How to Test:");
    println!("   1. Start a server on the server side: nc -l 8888");
    println!("   2. Connect from client side: nc 127.0.0.1 7777");
    println!("   3. Type messages in either terminal - they should tunnel through!");
    println!();
    println!("🔀 Active Tunnels:");
    println!("   Client 127.0.0.1:7777 ──┐");
    println!("                           │ (P2P tunnel)");
    println!("   Server 127.0.0.1:8888 ──┘");
    println!();
    println!("💡 The program will keep running until you press Ctrl+C");
    println!("   You can now test TCP tunneling with real network traffic!");

    // Wait for ctrl-c
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to listen for ctrl-c");
    println!("👋 Shutting down...");
}
