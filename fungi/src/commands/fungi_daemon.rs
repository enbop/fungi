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

    log::info!("Starting Fungi daemon...");

    let daemon = match FungiDaemon::start(common.fungi_dir(), args.clone()).await {
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
    let rpc_socket_addr = match rpc_listen_address
        .parse()
        .with_context(|| format!("Invalid RPC listen address: {rpc_listen_address}"))
    {
        Ok(addr) => addr,
        Err(error) => {
            print_startup_error("Failed to parse daemon RPC listen address", &error);
            return Err(error);
        }
    };
    let server_fut = start_grpc_server(daemon, rpc_socket_addr);

    let stdin_monitor = if args.exit_on_stdin_close {
        Some(tokio::spawn(stdin_monitor()))
    } else {
        None
    };

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            log::info!("Received Ctrl+C, shutting down Fungi daemon...");
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
