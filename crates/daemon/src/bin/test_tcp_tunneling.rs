use fungi_config::FungiConfig;
use fungi_daemon::FungiDaemon;
use libp2p::identity::Keypair;
use tempfile::TempDir;

const CLIENT_TCP_PORT: u16 = 50030;
const SERVER_TCP_PORT: u16 = 50040;

// Create client and server daemons for TCP tunneling testing
async fn create_tcp_tunneling_daemons() -> (FungiDaemon, FungiDaemon, TempDir, TempDir) {
    let client_key = Keypair::generate_secp256k1();
    let server_key = Keypair::generate_secp256k1();

    let client_peer_id = client_key.public().to_peer_id();
    let server_peer_id = server_key.public().to_peer_id();

    let client_temp_dir = TempDir::new().unwrap();
    let server_temp_dir = TempDir::new().unwrap();

    // Server config
    let mut server_config = FungiConfig::apply_from_dir(server_temp_dir.path()).unwrap();
    server_config.network.incoming_allowed_peers.push(client_peer_id);
    server_config.network.listen_tcp_port = SERVER_TCP_PORT;
    server_config.network.listen_udp_port = SERVER_TCP_PORT + 1000;
    
    // Disable other services
    server_config.file_transfer.server.enabled = false;
    server_config.file_transfer.proxy_ftp.enabled = false;
    server_config.file_transfer.proxy_webdav.enabled = false;

    // Client config
    let mut client_config = FungiConfig::apply_from_dir(client_temp_dir.path()).unwrap();
    client_config.network.listen_tcp_port = CLIENT_TCP_PORT;
    client_config.network.listen_udp_port = CLIENT_TCP_PORT + 1000;
    
    // Disable other services
    client_config.file_transfer.server.enabled = false;
    client_config.file_transfer.proxy_ftp.enabled = false;
    client_config.file_transfer.proxy_webdav.enabled = false;

    let client_daemon = FungiDaemon::start_with(Default::default(), client_config, client_key)
        .await
        .expect("Failed to start client daemon");

    let server_daemon = FungiDaemon::start_with(Default::default(), server_config, server_key)
        .await
        .expect("Failed to start server daemon");

    // Connect client to server
    let server_peer_id = server_daemon.swarm_control().local_peer_id();
    let server_addr = format!(
        "/ip4/127.0.0.1/tcp/{}/p2p/{}",
        SERVER_TCP_PORT, server_peer_id
    );

    client_daemon
        .swarm_control()
        .invoke_swarm(move |swarm| {
            swarm.add_peer_address(server_peer_id, server_addr.parse().unwrap())
        })
        .await
        .expect("Failed to add server address to client");

    println!("✅ Client Daemon started on port {}", CLIENT_TCP_PORT);
    println!("✅ Server Daemon started on port {}", SERVER_TCP_PORT);
    println!("✅ Client peer ID: {}", client_daemon.swarm_control().local_peer_id());
    println!("✅ Server peer ID: {}", server_daemon.swarm_control().local_peer_id());

    (client_daemon, server_daemon, client_temp_dir, server_temp_dir)
}



#[tokio::main]
async fn main() {
    env_logger::init();

    println!("🚀 Starting TCP Tunneling Test Program");
    println!("=======================================");

    let (client_daemon, server_daemon, _client_temp, _server_temp) = 
        create_tcp_tunneling_daemons().await;

    let server_peer_id = server_daemon.swarm_control().local_peer_id();
    
    println!("🔧 Setting up test tunneling rules...");
    
    // Add a listening rule on server (port 8888 -> tunneled traffic)
    match server_daemon.add_tcp_listening_rule(
        "127.0.0.1".to_string(),
        8888,
        "/fungi/test/1.0.0".to_string(),
        vec![],
    ) {
        Ok(listen_rule_id) => {
            println!("✅ Added listening rule: {}", listen_rule_id);
            println!("   Server listening on: 127.0.0.1:8888 (protocol: /fungi/test/1.0.0)");
        }
        Err(e) => println!("❌ Failed to add listening rule: {}", e),
    }
    
    // Add a forwarding rule on client (port 7777 -> server:8888)
    match client_daemon.add_tcp_forwarding_rule(
        "127.0.0.1".to_string(),
        7777,
        server_peer_id.to_string(),
        "/fungi/test/1.0.0".to_string(),
    ) {
        Ok(forward_rule_id) => {
            println!("✅ Added forwarding rule: {}", forward_rule_id);
            println!("   Client forwarding: 127.0.0.1:7777 -> {} (127.0.0.1:8888)", server_peer_id);
        }
        Err(e) => println!("❌ Failed to add forwarding rule: {}", e),
    }

    println!();
    println!("🔗 TCP Tunnel Setup Complete!");
    println!("===============================");
    println!("📋 Test Setup:");
    println!("   Client peer ID: {}", client_daemon.swarm_control().local_peer_id());
    println!("   Server peer ID: {}", server_peer_id);
    println!("   Client TCP port: {}", CLIENT_TCP_PORT);
    println!("   Server TCP port: {}", SERVER_TCP_PORT);
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
