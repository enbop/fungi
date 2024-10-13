pub use fungi_daemon::DaemonArgs;
use fungi_daemon::FungiDaemon;
use libp2p::{multiaddr::Protocol, Multiaddr, PeerId};

pub async fn run(args: DaemonArgs) {
    fungi_config::init(&args).unwrap();

    println!("Starting Fungi daemon...");
    let mut daemon = FungiDaemon::new(args).await;
    daemon.start().await;

    println!("Local Peer ID: {}", daemon.fungi_swarm.local_peer_id());

    let network_info = daemon
        .fungi_swarm
        .invoke_swarm(|swarm| swarm.network_info())
        .await
        .unwrap();
    println!("Network info: {:?}", network_info);

    if let Err(e) = daemon
        .fungi_swarm
        .listen_relay(get_default_relay_addr())
        .await
    {
        eprintln!("Failed to listen on relay: {:?}", e);
    };
    tokio::signal::ctrl_c().await.ok();
    println!("Shutting down Fungi daemon...");
}

pub(crate) fn get_default_relay_addr() -> Multiaddr {
    "/ip4/160.16.206.21/tcp/30001/p2p/16Uiu2HAmGXFS6aYsKKYRkEDo1tNigZKN8TAYrsfSnEdC5sZLNkiE"
        .parse()
        .unwrap()
}

pub fn peer_addr_with_relay(peer_id: PeerId, relay: Multiaddr) -> Multiaddr {
    relay
        .with(Protocol::P2pCircuit)
        .with(Protocol::P2p(peer_id))
}
