use anyhow::Result;
pub use fungi_daemon::DaemonArgs;
use fungi_daemon::FungiDaemon;

pub async fn run(args: DaemonArgs) -> Result<()> {
    fungi_config::init(&args).unwrap();

    println!("Starting Fungi daemon...");

    let daemon = FungiDaemon::start(args).await?;

    let swarm_control = daemon.swarm_control().clone();
    println!("Local Peer ID: {}", swarm_control.local_peer_id());

    let network_info = swarm_control
        .invoke_swarm(|swarm| swarm.network_info())
        .await
        .unwrap();
    println!("Network info: {network_info:?}");

    if !daemon.config().lock().network.disable_relay {
        swarm_control.listen_relay().await;
    }

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            println!("Shutting down Fungi daemon...");
        },
        _ = daemon.wait_all() => {},
    }

    Ok(())
}
