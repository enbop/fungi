use clap::Subcommand;
use fungi_daemon_grpc::{
    Request,
    fungi_daemon_grpc::{
        Empty, RuntimeAllowedHostPathRequest, RuntimeAllowedPortRangeRequest,
        RuntimeAllowedPortRequest,
    },
};

use crate::commands::CommonArgs;

use super::{
    AllowedPeerCommands,
    allowed_peers::execute_allowed_peer,
    client::get_rpc_client,
    shared::{fatal, fatal_grpc, host_path_risk_note},
};

#[derive(Subcommand, Debug, Clone)]
pub enum SecurityCommands {
    /// Show current runtime safety boundary configuration
    Show,
    /// Manage peers allowed to initiate incoming connections
    #[command(subcommand)]
    AllowedPeers(AllowedPeerCommands),
    /// Add an allowed host path root
    AllowPath {
        /// Absolute host path root
        path: String,
    },
    /// Remove an allowed host path root
    DenyPath {
        /// Absolute host path root
        path: String,
    },
    /// Add an allowed host port
    AllowPort {
        /// TCP port
        port: u16,
    },
    /// Remove an allowed host port
    DenyPort {
        /// TCP port
        port: u16,
    },
    /// Add an allowed host port range
    AllowRange {
        /// Inclusive start port
        start: u16,
        /// Inclusive end port
        end: u16,
    },
    /// Remove an allowed host port range
    DenyRange {
        /// Inclusive start port
        start: u16,
        /// Inclusive end port
        end: u16,
    },
}

pub async fn execute_security(args: CommonArgs, cmd: SecurityCommands) {
    if let SecurityCommands::AllowedPeers(subcmd) = cmd {
        execute_allowed_peer(args, subcmd).await;
        return;
    }

    let mut client = match get_rpc_client(&args).await {
        Some(c) => c,
        None => fatal("Cannot connect to Fungi daemon. Is it running?"),
    };

    match cmd {
        SecurityCommands::Show => match client.get_runtime_config(Request::new(Empty {})).await {
            Ok(resp) => {
                let config = resp.into_inner();
                println!("disable_docker: {}", config.disable_docker);
                println!("disable_wasmtime: {}", config.disable_wasmtime);
                println!("allowed_host_paths:");
                for path in config.allowed_host_paths {
                    println!("  {}", path);
                    if let Some(note) = host_path_risk_note(&path) {
                        println!("  ! {}", note);
                    }
                }
                println!("allowed_ports:");
                for port in config.allowed_ports {
                    println!("  {}", port);
                }
                println!("allowed_port_ranges:");
                for range in config.allowed_port_ranges {
                    println!("  {}-{}", range.start, range.end);
                }
            }
            Err(e) => fatal_grpc(e),
        },
        SecurityCommands::AllowedPeers(_) => unreachable!(),
        SecurityCommands::AllowPath { path } => {
            if let Some(note) = host_path_risk_note(&path) {
                eprintln!("Warning: {}", note);
            }
            let req = RuntimeAllowedHostPathRequest { path };
            match client
                .add_runtime_allowed_host_path(Request::new(req))
                .await
            {
                Ok(_) => println!("Allowed host path added"),
                Err(e) => fatal_grpc(e),
            }
        }
        SecurityCommands::DenyPath { path } => {
            let req = RuntimeAllowedHostPathRequest { path };
            match client
                .remove_runtime_allowed_host_path(Request::new(req))
                .await
            {
                Ok(_) => println!("Allowed host path removed"),
                Err(e) => fatal_grpc(e),
            }
        }
        SecurityCommands::AllowPort { port } => {
            let req = RuntimeAllowedPortRequest {
                port: i32::from(port),
            };
            match client.add_runtime_allowed_port(Request::new(req)).await {
                Ok(_) => println!("Allowed port added"),
                Err(e) => fatal_grpc(e),
            }
        }
        SecurityCommands::DenyPort { port } => {
            let req = RuntimeAllowedPortRequest {
                port: i32::from(port),
            };
            match client.remove_runtime_allowed_port(Request::new(req)).await {
                Ok(_) => println!("Allowed port removed"),
                Err(e) => fatal_grpc(e),
            }
        }
        SecurityCommands::AllowRange { start, end } => {
            let req = RuntimeAllowedPortRangeRequest {
                start: i32::from(start),
                end: i32::from(end),
            };
            match client
                .add_runtime_allowed_port_range(Request::new(req))
                .await
            {
                Ok(_) => println!("Allowed port range added"),
                Err(e) => fatal_grpc(e),
            }
        }
        SecurityCommands::DenyRange { start, end } => {
            let req = RuntimeAllowedPortRangeRequest {
                start: i32::from(start),
                end: i32::from(end),
            };
            match client
                .remove_runtime_allowed_port_range(Request::new(req))
                .await
            {
                Ok(_) => println!("Allowed port range removed"),
                Err(e) => fatal_grpc(e),
            }
        }
    }
}
