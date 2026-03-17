use clap::Subcommand;
use fungi_daemon_grpc::{
    Request,
    fungi_daemon_grpc::{Empty, StartFileTransferServiceRequest},
};

use crate::commands::CommonArgs;

use super::{
    client::get_rpc_client,
    shared::{fatal, fatal_grpc},
};

#[derive(Subcommand, Debug, Clone)]
pub enum FtServiceCommands {
    /// Show file transfer service status
    Status,
    /// Start the file transfer service
    Start {
        /// Root directory to share
        #[arg(short, long)]
        root_dir: String,
    },
    /// Stop the file transfer service
    Stop,
}

pub async fn execute_ft_service(args: CommonArgs, cmd: FtServiceCommands) {
    let mut client = match get_rpc_client(&args).await {
        Some(c) => c,
        None => fatal("Cannot connect to Fungi daemon. Is it running?"),
    };

    match cmd {
        FtServiceCommands::Status => {
            match client
                .get_file_transfer_service_enabled(Request::new(Empty {}))
                .await
            {
                Ok(resp) => {
                    let enabled = resp.into_inner().enabled;
                    println!(
                        "File transfer service: {}",
                        if enabled { "running" } else { "stopped" }
                    );

                    if enabled
                        && let Ok(dir_resp) = client
                            .get_file_transfer_service_root_dir(Request::new(Empty {}))
                            .await
                    {
                        println!("Root directory: {}", dir_resp.into_inner().root_dir);
                    }
                }
                Err(e) => fatal_grpc(e),
            }
        }
        FtServiceCommands::Start { root_dir } => {
            let req = StartFileTransferServiceRequest { root_dir };
            match client.start_file_transfer_service(Request::new(req)).await {
                Ok(_) => println!("File transfer service started"),
                Err(e) => fatal_grpc(e),
            }
        }
        FtServiceCommands::Stop => {
            match client
                .stop_file_transfer_service(Request::new(Empty {}))
                .await
            {
                Ok(_) => println!("File transfer service stopped"),
                Err(e) => fatal_grpc(e),
            }
        }
    }
}
