use clap::Subcommand;
use fungi_daemon_grpc::{
    Request,
    fungi_daemon_grpc::{Empty, RuntimeAllowedHostPathRequest},
};

use crate::commands::CommonArgs;

use super::{
    client::get_rpc_client,
    shared::{fatal, fatal_grpc, host_path_risk_note},
};

#[derive(Subcommand, Debug, Clone)]
pub enum SecurityCommands {
    /// Show current runtime safety boundary configuration
    Show,
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
}

pub async fn execute_security(args: CommonArgs, cmd: SecurityCommands) {
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
            }
            Err(e) => fatal_grpc(e),
        },
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
    }
}
