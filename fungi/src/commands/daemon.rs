use fungi_daemon::{DaemonArgs, FungiDaemon, ALL_IN_ONE_BINARY};

pub async fn daemon(args: DaemonArgs, all_in_one_binary: bool) {
    ALL_IN_ONE_BINARY.set(all_in_one_binary).unwrap();
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
