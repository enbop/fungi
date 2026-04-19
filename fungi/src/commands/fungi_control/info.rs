use clap::Subcommand;
use fungi_config::{FungiConfig, FungiDir};
use fungi_daemon_grpc::{Request, fungi_daemon_grpc::Empty};
use serde_json::json;

use crate::commands::CommonArgs;

use super::{
    client::get_rpc_client,
    shared::{fatal, fatal_grpc},
};

#[derive(Subcommand, Debug, Clone)]
pub enum InfoCommands {
    /// Show daemon version
    Version,
    /// Show local binary build information
    Build {
        /// Print build information as JSON
        #[arg(long)]
        json: bool,
    },
    /// Show peer ID of this daemon
    Id,
    /// Show hostname of this device
    Hostname,
    /// Show configuration file path
    ConfigPath,
    /// Show RPC address
    RpcAddress,
    /// Show local runtime status observed by the daemon
    Runtime,
}

pub async fn execute_info(args: CommonArgs, cmd: InfoCommands) {
    if let InfoCommands::Build { json } = cmd {
        print_build_info(json);
        return;
    }

    if matches!(cmd, InfoCommands::RpcAddress) {
        let fungi_config = FungiConfig::try_read_from_dir(&args.fungi_dir())
            .unwrap_or_else(|error| fatal(format!("Failed to read configuration: {error}")));
        println!("{}", fungi_config.rpc.listen_address);
        return;
    }

    if matches!(cmd, InfoCommands::ConfigPath) {
        let _fungi_config = FungiConfig::try_read_from_dir(&args.fungi_dir())
            .unwrap_or_else(|error| fatal(format!("Failed to read configuration: {error}")));
        let mut client = match get_rpc_client(&args).await {
            Some(c) => c,
            None => fatal("Cannot connect to Fungi daemon. Is it running?"),
        };
        match client.config_file_path(Request::new(Empty {})).await {
            Ok(resp) => println!("{}", resp.into_inner().config_file_path),
            Err(e) => fatal_grpc(e),
        }
        return;
    }

    let mut client = match get_rpc_client(&args).await {
        Some(c) => c,
        None => fatal("Cannot connect to Fungi daemon. Is it running?"),
    };

    match cmd {
        InfoCommands::Version => match client.version(Request::new(Empty {})).await {
            Ok(resp) => println!("{}", resp.into_inner().version),
            Err(e) => fatal_grpc(e),
        },
        InfoCommands::Id => match client.peer_id(Request::new(Empty {})).await {
            Ok(resp) => println!("{}", resp.into_inner().peer_id),
            Err(e) => fatal_grpc(e),
        },
        InfoCommands::Hostname => match client.hostname(Request::new(Empty {})).await {
            Ok(resp) => println!("{}", resp.into_inner().hostname),
            Err(e) => fatal_grpc(e),
        },
        InfoCommands::Runtime => match client
            .get_local_runtime_status(Request::new(Empty {}))
            .await
        {
            Ok(resp) => print_runtime_status(&resp.into_inner()),
            Err(e) => fatal_grpc(e),
        },
        InfoCommands::ConfigPath | InfoCommands::RpcAddress | InfoCommands::Build { .. } => {}
    }
}

fn print_build_info(json_output: bool) {
    if json_output {
        println!(
            "{}",
            json!({
                "version": env!("CARGO_PKG_VERSION"),
                "channel": fungi_config::dist_channel(),
                "commit": fungi_config::build_commit(),
                "build_time": fungi_config::build_time(),
                "default_fungi_dir": fungi_config::default_fungi_dir_name(),
                "default_rpc_address": fungi_config::default_rpc_address(),
            })
        );
        return;
    }

    println!("version: {}", env!("CARGO_PKG_VERSION"));
    println!("channel: {}", fungi_config::dist_channel());
    println!("commit: {}", fungi_config::build_commit());
    println!("build_time: {}", fungi_config::build_time());
    println!(
        "default_fungi_dir: {}",
        fungi_config::default_fungi_dir_name()
    );
    println!(
        "default_rpc_address: {}",
        fungi_config::default_rpc_address()
    );
}

fn print_runtime_status(status: &fungi_daemon_grpc::fungi_daemon_grpc::LocalRuntimeStatusResponse) {
    print_runtime_entry("docker", status.docker.as_ref());
    print_runtime_entry("wasmtime", status.wasmtime.as_ref());
}

fn print_runtime_entry(
    name: &str,
    status: Option<&fungi_daemon_grpc::fungi_daemon_grpc::RuntimeAvailabilityStatus>,
) {
    println!("{name}:");
    let Some(status) = status else {
        println!("  <unavailable>");
        return;
    };

    println!("  config_enabled: {}", status.config_enabled);
    println!("  detected: {}", status.detected);
    println!("  active: {}", status.active);
    if !status.endpoint.is_empty() {
        println!("  endpoint: {}", status.endpoint);
    }
}
