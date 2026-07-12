use crate::commands::CommonArgs;
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use fungi_config::FungiDir;
use fungi_daemon::FungiDaemon;
use fungi_daemon_grpc::start_grpc_server;

use super::fungi_relay::RelayArgs;

#[derive(Debug, Clone, Parser)]
pub struct DaemonCommandArgs {
    #[command(subcommand)]
    pub subcommand: Option<DaemonSubcommand>,

    #[command(flatten)]
    pub daemon_args: fungi_daemon::DaemonArgs,
}

#[derive(Debug, Clone, Subcommand)]
pub enum DaemonSubcommand {
    /// Start a simple Fungi relay server
    RelayServer(RelayArgs),
}

pub async fn execute(common: CommonArgs, args: DaemonCommandArgs) -> Result<()> {
    match args.subcommand {
        Some(DaemonSubcommand::RelayServer(relay_args)) => {
            super::fungi_relay::run(relay_args).await
        }
        None => run(common, args.daemon_args).await,
    }
}

pub async fn run(common: CommonArgs, args: fungi_daemon::DaemonArgs) -> Result<()> {
    if let Err(error) = fungi_config::init(&common, false) {
        print_startup_error("Failed to initialize Fungi configuration", &error);
        return Err(error);
    }

    let fungi_dir = common.fungi_dir();
    let _instance_lock = match fungi_config::DaemonInstanceLock::acquire(&fungi_dir) {
        Ok(lock) => lock,
        Err(error) => {
            print_startup_error("Failed to acquire daemon instance lock", &error);
            return Err(error);
        }
    };

    log::info!("Starting Fungi daemon...");

    let daemon = match FungiDaemon::start(fungi_dir.clone(), args.clone()).await {
        Ok(daemon) => daemon,
        Err(error) => {
            print_startup_error("Failed to start Fungi daemon", &error);
            return Err(error);
        }
    };

    let swarm_control = daemon.swarm_control().clone();
    log::info!("Local Peer ID: {}", swarm_control.local_peer_id());

    let network_info = swarm_control
        .invoke_swarm(|swarm| swarm.network_info())
        .await
        .unwrap();
    log::info!("Network info: {network_info:?}");

    let rpc_listen_address = daemon.config().lock().rpc.listen_address.clone();
    let rpc_listener = match bind_rpc_listener(&rpc_listen_address).await {
        Ok(listener) => listener,
        Err(error) => {
            print_grpc_startup_error(&rpc_listen_address, &error);
            return Err(error);
        }
    };
    let rpc_socket_addr = rpc_listener
        .local_addr()
        .context("Failed to read daemon RPC listener address")?;
    let _published_endpoint =
        fungi_config::PublishedDaemonEndpoint::publish(&fungi_dir, rpc_socket_addr)?;
    log::info!("Daemon RPC endpoint: http://{rpc_socket_addr}");
    let server_fut = start_grpc_server(daemon, rpc_listener);

    let stdin_monitor = if args.exit_on_stdin_close {
        Some(tokio::spawn(stdin_monitor()))
    } else {
        None
    };

    tokio::select! {
        signal = termination_signal() => {
            let signal = signal.context("Failed to wait for daemon termination signal")?;
            log::info!("Received {signal}, shutting down Fungi daemon...");
        },
        res = server_fut => {
            if let Err(error) = res {
                print_grpc_startup_error(&rpc_listen_address, &error);
                log::error!("Error occurred while serving: {}", error);
                return Err(error);
            }
        },
        _ = async {
            if let Some(monitor) = stdin_monitor {
                let _ = monitor.await;
            } else {
                std::future::pending::<()>().await
            }
        } => {
            log::info!("Shutting down Fungi daemon...");
        },
    }

    Ok(())
}

async fn bind_rpc_listener(listen_address: &str) -> Result<tokio::net::TcpListener> {
    let socket_addr: std::net::SocketAddr = listen_address
        .parse()
        .with_context(|| format!("Invalid RPC listen address: {listen_address}"))?;
    tokio::net::TcpListener::bind(socket_addr)
        .await
        .with_context(|| format!("Failed to bind daemon RPC listener: {listen_address}"))
}

#[cfg(unix)]
async fn termination_signal() -> std::io::Result<&'static str> {
    use tokio::signal::unix::{SignalKind, signal};

    let mut terminate = signal(SignalKind::terminate())?;
    tokio::select! {
        result = tokio::signal::ctrl_c() => {
            result?;
            Ok("Ctrl+C")
        },
        _ = terminate.recv() => Ok("SIGTERM"),
    }
}

#[cfg(not(unix))]
async fn termination_signal() -> std::io::Result<&'static str> {
    tokio::signal::ctrl_c().await?;
    Ok("Ctrl+C")
}

fn print_grpc_startup_error(rpc_listen_address: &str, error: &anyhow::Error) {
    if error_chain_contains(error, "address already in use")
        || error_chain_contains(error, "addrinuse")
    {
        println!(
            "Failed to start daemon RPC server on {}: address already in use.",
            rpc_listen_address
        );
        println!(
            "Another process is already listening on that port, and it is often an already-running fungi daemon."
        );
        print_error_reasons(error);
        return;
    }

    println!(
        "Failed to start daemon RPC server on {}.",
        rpc_listen_address
    );
    print_error_reasons(error);
}

fn print_startup_error(summary: &str, error: &anyhow::Error) {
    println!("{summary}.");
    print_error_reasons(error);
}

fn print_error_reasons(error: &anyhow::Error) {
    println!("Reason: {}", error);
    for cause in error.chain().skip(1) {
        println!("Caused by: {}", cause);
    }
}

fn error_chain_contains(error: &anyhow::Error, needle: &str) -> bool {
    let needle = needle.to_ascii_lowercase();
    error
        .chain()
        .any(|cause| cause.to_string().to_ascii_lowercase().contains(&needle))
}

// Monitor stdin for EOF to detect parent process termination
async fn stdin_monitor() {
    use tokio::io::AsyncReadExt;
    let mut stdin = tokio::io::stdin();
    let mut buf = [0u8; 64];

    loop {
        match stdin.read(&mut buf).await {
            Ok(0) => {
                log::info!("Stdin closed, parent process likely terminated. Shutting down...");
                break;
            }
            Ok(_) => {
                // Ignore any input data
                continue;
            }
            Err(e) => {
                log::error!("Error reading stdin: {}, shutting down...", e);
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::bind_rpc_listener;

    #[tokio::test]
    async fn dynamic_rpc_listener_reports_an_allocated_port() {
        let listener = bind_rpc_listener("127.0.0.1:0").await.unwrap();
        assert_ne!(listener.local_addr().unwrap().port(), 0);
    }
}
