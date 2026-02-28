use clap::Subcommand;
use fungi_config::{FungiConfig, FungiDir};
use fungi_daemon_grpc::{Request, fungi_daemon_grpc::Empty};

use crate::commands::CommonArgs;

use super::client::get_rpc_client;

#[derive(Subcommand, Debug, Clone)]
pub enum InfoCommands {
    /// Show daemon version
    Version,
    /// Show peer ID of this daemon
    Id,
    /// Show hostname of this device
    Hostname,
    /// Show configuration file path
    ConfigPath,
    /// Show RPC address
    RpcAddress,
}

pub async fn execute_info(args: CommonArgs, cmd: InfoCommands) {
    if matches!(cmd, InfoCommands::RpcAddress) {
        let Ok(fungi_config) = FungiConfig::try_read_from_dir(&args.fungi_dir()) else {
            eprintln!("Failed to read configuration");
            return;
        };
        println!("{}", fungi_config.rpc.listen_address);
        return;
    }

    if matches!(cmd, InfoCommands::ConfigPath) {
        let Ok(_fungi_config) = FungiConfig::try_read_from_dir(&args.fungi_dir()) else {
            eprintln!("Failed to read configuration");
            return;
        };
        let mut client = match get_rpc_client(&args).await {
            Some(c) => c,
            None => return,
        };
        match client.config_file_path(Request::new(Empty {})).await {
            Ok(resp) => println!("{}", resp.into_inner().config_file_path),
            Err(e) => eprintln!("Error: {}", e),
        }
        return;
    }

    let mut client = match get_rpc_client(&args).await {
        Some(c) => c,
        None => return,
    };

    match cmd {
        InfoCommands::Version => match client.version(Request::new(Empty {})).await {
            Ok(resp) => println!("{}", resp.into_inner().version),
            Err(e) => eprintln!("Error: {}", e),
        },
        InfoCommands::Id => match client.peer_id(Request::new(Empty {})).await {
            Ok(resp) => println!("{}", resp.into_inner().peer_id),
            Err(e) => eprintln!("Error: {}", e),
        },
        InfoCommands::Hostname => match client.hostname(Request::new(Empty {})).await {
            Ok(resp) => println!("{}", resp.into_inner().hostname),
            Err(e) => eprintln!("Error: {}", e),
        },
        InfoCommands::ConfigPath | InfoCommands::RpcAddress => {}
    }
}
