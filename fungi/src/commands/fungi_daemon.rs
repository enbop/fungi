use anyhow::Result;
pub use fungi_daemon::DaemonArgs;
use fungi_daemon::FungiDaemon;
use fungi_daemon_grpc::start_grpc_server;

pub async fn run(args: DaemonArgs) -> Result<()> {
    fungi_config::init(&args).unwrap();

    println!("Starting Fungi daemon...");

    let daemon = FungiDaemon::start(args.clone()).await?;

    let swarm_control = daemon.swarm_control().clone();
    println!("Local Peer ID: {}", swarm_control.local_peer_id());

    let network_info = swarm_control
        .invoke_swarm(|swarm| swarm.network_info())
        .await
        .unwrap();
    println!("Network info: {network_info:?}");

    let rpc_listen_address = daemon.config().lock().rpc.listen_address.clone();
    let server_fut = start_grpc_server(daemon, rpc_listen_address.parse().unwrap());

    let stdin_monitor = if args.exit_on_stdin_close {
        Some(tokio::spawn(stdin_monitor()))
    } else {
        None
    };

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            println!("Received Ctrl+C, shutting down Fungi daemon...");
        },
        res = server_fut => {
            if let Err(e) = res {
                eprintln!("Error occurred while serving: {}", e);
            }
        },
        _ = async {
            if let Some(monitor) = stdin_monitor {
                let _ = monitor.await;
            } else {
                std::future::pending::<()>().await
            }
        } => {
            println!("Shutting down Fungi daemon...");
        },
    }

    Ok(())
}

// Monitor stdin for EOF to detect parent process termination
async fn stdin_monitor() {
    use tokio::io::AsyncReadExt;
    let mut stdin = tokio::io::stdin();
    let mut buf = [0u8; 64];

    loop {
        match stdin.read(&mut buf).await {
            Ok(0) => {
                println!("Stdin closed, parent process likely terminated. Shutting down...");
                break;
            }
            Ok(_) => {
                // Ignore any input data
                continue;
            }
            Err(e) => {
                eprintln!("Error reading stdin: {}, shutting down...", e);
                break;
            }
        }
    }
}
