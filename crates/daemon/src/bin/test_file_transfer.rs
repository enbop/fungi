use fungi_config::FungiConfig;
use fungi_daemon::FungiDaemon;
use fungi_swarm::get_default_relay_addr;
use libp2p::identity::Keypair;
use std::path::PathBuf;

const SERVER_TCP_PORT: u16 = 50010;

// (client_daemon, server_daemon)
async fn create_daemons() -> (FungiDaemon, FungiDaemon) {
    let client_key = Keypair::generate_secp256k1();
    let server_key = Keypair::generate_secp256k1();

    let client_peer_id = client_key.public().to_peer_id();
    let server_peer_id = server_key.public().to_peer_id();

    let mut server_config = FungiConfig::default();
    server_config
        .network
        .incoming_allowed_peers
        .push(client_peer_id);
    server_config.network.listen_tcp_port = SERVER_TCP_PORT;
    server_config.file_transfer.server.enabled = true;
    server_config.file_transfer.server.shared_root_dir = PathBuf::from(".");

    server_config.file_transfer.proxy_ftp.enabled = false;
    server_config.file_transfer.proxy_webdav.enabled = false;

    let mut client_config = FungiConfig::default();
    client_config.file_transfer.proxy_webdav.host = "0.0.0.0".parse().unwrap();
    client_config.file_transfer.proxy_ftp.host = "0.0.0.0".parse().unwrap();
    client_config
        .file_transfer
        .client
        .push(fungi_config::file_transfer::FileTransferClient {
            enabled: true,
            name: Some("Test".to_string()),
            peer_id: server_peer_id,
        });

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
    let server_addr = format!("/ip4/127.0.0.1/tcp/{SERVER_TCP_PORT}/p2p/{server_peer_id}");

    // server_daemon
    //     .swarm_control()
    //     .listen_relay(get_default_relay_addr())
    //     .await
    //     .unwrap();

    client_daemon
        .swarm_control()
        .invoke_swarm(move |swarm| {
            swarm.add_peer_address(server_peer_id, server_addr.parse().unwrap())
        })
        .await
        .expect("Failed to get client network info");

    (client_daemon, server_daemon)
}

#[tokio::main]
async fn main() {
    env_logger::init();

    println!("🚀 Starting File Transfer Test Program");
    println!("======================================");

    let (client_daemon, server_daemon) = create_daemons().await;

    let server_peer_id = server_daemon.swarm_control().local_peer_id();
    let client_peer_id = client_daemon.swarm_control().local_peer_id();

    println!("✅ File transfer daemons started successfully!");
    println!("📋 Server peer ID: {server_peer_id}");
    println!("📋 Client peer ID: {client_peer_id}");
    println!("📋 Server TCP port: {SERVER_TCP_PORT}");
    println!();

    // Show WebDAV info
    let webdav_config = client_daemon
        .config()
        .lock()
        .clone()
        .file_transfer
        .proxy_webdav;
    if webdav_config.enabled {
        println!("🌐 WebDAV Proxy Available:");
        println!(
            "   URL: http://{}:{}/",
            webdav_config.host, webdav_config.port
        );
        println!("   Access via: Finder (macOS), Explorer (Windows), or any WebDAV client");
    }

    // Show FTP info
    let ftp_config = client_daemon
        .config()
        .lock()
        .clone()
        .file_transfer
        .proxy_ftp;
    if ftp_config.enabled {
        println!("📡 FTP Proxy Available:");
        println!("   Host: {}:{}", ftp_config.host, ftp_config.port);
        println!("   Access via: FileZilla, WinSCP, or any FTP client (anonymous login)");
    }

    println!(
        "📁 Server shared directory: {:?}",
        server_daemon.get_file_transfer_service_root_dir()
    );
    println!();
    println!("💡 The program will keep running until you press Ctrl+C");
    println!("   You can now test file transfer using WebDAV/FTP clients!");

    // Wait for ctrl-c
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to listen for ctrl-c");
    println!("👋 Shutting down...");
}
