use anyhow::Result;
pub use fungi_daemon::DaemonArgs;
use fungi_daemon::FungiDaemon;
use libp2p::{multiaddr::Protocol, Multiaddr, PeerId};

pub async fn run(args: DaemonArgs) -> Result<()> {
    fungi_config::init(&args).unwrap();

    println!("Starting Fungi daemon...");

    let daemon = FungiDaemon::start(args).await?;

    let swarm_controller = daemon.swarm_controller.clone();
    println!("Local Peer ID: {}", swarm_controller.local_peer_id());

    let network_info = swarm_controller
        .invoke_swarm(|swarm| swarm.network_info())
        .await
        .unwrap();
    println!("Network info: {:?}", network_info);

    if let Err(e) = swarm_controller
        .listen_relay(get_default_relay_addr())
        .await
    {
        eprintln!("Failed to listen on relay: {:?}", e);
    };

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            println!("Shutting down Fungi daemon...");
        },
        _ = daemon.wait_all() => {},
    }

    Ok(())
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
