use clap::Subcommand;
use fungi_config::{FungiConfig, FungiDir};
use fungi_daemon_grpc::{
    Request,
    fungi_daemon_grpc::{Empty, fungi_daemon_client::FungiDaemonClient},
};

use crate::commands::CommonArgs;

#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
pub enum ControlCommands {
    /// Show hostname of this device
    Hostname,
    /// Show Peer ID
    Id,
    /// Show info of this Fungi daemon
    Info,
    /// Show RPC address
    RpcAddress,
}

pub async fn execute(args: CommonArgs, cmd: ControlCommands) {
    let Ok(fungi_config) = FungiConfig::try_read_from_dir(&args.fungi_dir()) else {
        eprintln!("Failed to read Fungi configuration. Please ensure it is initialized.");
        return;
    };

    if cmd == ControlCommands::RpcAddress {
        println!("{}", fungi_config.rpc.listen_address);
        return;
    }

    let mut rpc_client = match FungiDaemonClient::connect("http://[::1]:50051").await {
        Ok(client) => client,
        Err(e) => {
            eprintln!("Cannot connect to Fungi daemon. Is it running?");
            log::error!("Error occurred while connecting to Fungi daemon: {}", e);
            return;
        }
    };

    match cmd {
        ControlCommands::Hostname => {
            let request = Request::new(Empty {});
            let response = rpc_client.hostname(request).await.unwrap();
            println!("{}", response.into_inner().hostname);
        }
        ControlCommands::Id => {
            let request = Request::new(Empty {});
            let response = rpc_client.peer_id(request).await.unwrap();
            println!("{}", response.into_inner().peer_id);
        }
        ControlCommands::Info => {
            let request = Request::new(Empty {});
            let response = rpc_client.version(request).await.unwrap();
            println!("Version: {}", response.into_inner().version);
        }
        ControlCommands::RpcAddress => {
            // Already handled above
        }
    }
}
