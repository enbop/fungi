use fungi_config::file_transfer::FileTransferClient;
use fungi_daemon::test_support::{TestDaemon, TestDaemonBuilder};
use libp2p::identity::Keypair;
use std::path::PathBuf;

// (client_daemon, server_daemon)
async fn create_daemons() -> (TestDaemon, TestDaemon) {
    let client_key = Keypair::generate_secp256k1();
    let server_key = Keypair::generate_secp256k1();

    let client_peer_id = client_key.public().to_peer_id();
    let server_peer_id = server_key.public().to_peer_id();

    let server_daemon = TestDaemonBuilder::new()
        .with_keypair(server_key)
        .with_trusted_device(client_peer_id)
        .with_config(|cfg| {
            cfg.file_transfer.server.enabled = true;
            cfg.file_transfer.server.shared_root_dir = PathBuf::from("docs");
        })
        .build()
        .await
        .expect("Failed to start server daemon");

    let client_daemon = TestDaemonBuilder::new()
        .with_keypair(client_key)
        .with_trusted_device(server_peer_id)
        .with_config(move |cfg| {
            cfg.file_transfer.proxy_webdav.host = "0.0.0.0".parse().unwrap();
            cfg.file_transfer.proxy_ftp.host = "0.0.0.0".parse().unwrap();
            cfg.file_transfer.client.push(FileTransferClient {
                enabled: true,
                name: Some("Test".to_string()),
                peer_id: server_peer_id,
            });
        })
        .build()
        .await
        .expect("Failed to start client daemon");

    client_daemon
        .connect_to(&server_daemon)
        .await
        .expect("Failed to connect client to server");

    (client_daemon, server_daemon)
}

#[tokio::main]
async fn main() {
    env_logger::init();

    println!("🚀 Starting File Transfer Test Program");
    println!("======================================");

    let (client_daemon, server_daemon) = create_daemons().await;

    let server_peer_id = server_daemon.peer_id();
    let client_peer_id = client_daemon.peer_id();

    println!("✅ File transfer daemons started successfully!");
    println!("📋 Server peer ID: {server_peer_id}");
    println!("📋 Client peer ID: {client_peer_id}");
    println!("📋 Server TCP port: {}", server_daemon.tcp_port);
    println!();

    // Show WebDAV info
    let webdav_config = client_daemon
        .daemon()
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
        .daemon()
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
        server_daemon.daemon().get_file_transfer_service_root_dir()
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
