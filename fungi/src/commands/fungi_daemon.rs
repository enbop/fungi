pub use fungi_daemon::DaemonArgs;
use fungi_daemon::FungiDaemon;

pub async fn run(args: DaemonArgs) {
    fungi_config::init(&args).unwrap();

    println!("Starting Fungi daemon...");
    let mut daemon = FungiDaemon::new(args).await;
    daemon.start().await;

    println!("Local Peer ID: {}", daemon.swarm_daemon.local_peer_id());

    let network_info = daemon
        .swarm_daemon
        .invoke_swarm(|swarm| swarm.network_info())
        .await
        .unwrap();
    println!("Network info: {:?}", network_info);

    tokio::signal::ctrl_c().await.ok();
    println!("Shutting down Fungi daemon...");
}
