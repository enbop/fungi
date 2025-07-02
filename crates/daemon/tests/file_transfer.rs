use std::path::PathBuf;

use fungi_config::FungiConfig;
use fungi_daemon::FungiDaemon;
use libp2p::identity::Keypair;

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
    server_config.file_transfer.server.shared_root_dir = PathBuf::from("/tmp"); // TODO use a temporary directory

    server_config.file_transfer.proxy_ftp.enabled = false;
    server_config.file_transfer.proxy_webdav.enabled = false;

    let mut client_config = FungiConfig::default();
    client_config
        .file_transfer
        .client
        .push(fungi_config::file_transfer::FileTransferClient {
            enabled: true,
            name: Some("Test".to_string()),
            peer_id: server_peer_id,
        });

    let client_daemon = FungiDaemon::start_with(Default::default(), client_config, client_key)
        .await
        .expect("Failed to start client daemon");

    let server_daemon = FungiDaemon::start_with(Default::default(), server_config, server_key)
        .await
        .expect("Failed to start server daemon");
    (client_daemon, server_daemon)
}

#[tokio::test]
async fn main() {
    env_logger::init();

    let (client_daemon, server_daemon) = create_daemons().await;

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
        .expect("Failed to get client network info");

    // wait for ctl-c
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to listen for ctrl-c");
    println!("Shutting down...");
}
