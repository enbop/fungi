use clap::{Args, Subcommand};
use fungi_daemon_grpc::{
    Request,
    fungi_daemon_grpc::{
        AddFileTransferClientRequest, Empty, EnableFileTransferClientRequest,
        RemoveFileTransferClientRequest, UpdateFtpProxyRequest, UpdateWebdavProxyRequest,
    },
};

use crate::commands::CommonArgs;

use super::{
    client::get_rpc_client,
    shared::{fatal, fatal_grpc},
};

#[derive(Args, Debug, Clone)]
pub struct AddClientArgs {
    /// Peer ID of the client
    peer_id: String,
    /// Display name for the client
    #[arg(short, long)]
    name: Option<String>,
    /// Enable the client immediately
    #[arg(short, long, default_value_t = false)]
    enabled: bool,
}

#[derive(Subcommand, Debug, Clone)]
pub enum FtClientCommands {
    /// List all file transfer clients
    List,
    /// Add a new file transfer client
    Add(AddClientArgs),
    /// Remove a file transfer client
    Remove {
        /// Peer ID of the client
        peer_id: String,
    },
    /// Enable or disable a file transfer client
    Enable {
        /// Peer ID of the client
        peer_id: String,
        /// Whether to enable the client
        #[arg(short, long)]
        enabled: bool,
    },
    /// Show FTP proxy settings
    FtpStatus,
    /// Update FTP proxy configuration
    FtpUpdate {
        /// Enable or disable FTP proxy
        #[arg(short, long)]
        enabled: bool,
        /// Host to bind to
        #[arg(short = 'H', long)]
        host: String,
        /// Port to bind to
        #[arg(short, long)]
        port: u16,
    },
    /// Show WebDAV proxy settings
    WebdavStatus,
    /// Update WebDAV proxy configuration
    WebdavUpdate {
        /// Enable or disable WebDAV proxy
        #[arg(short, long)]
        enabled: bool,
        /// Host to bind to
        #[arg(short = 'H', long)]
        host: String,
        /// Port to bind to
        #[arg(short, long)]
        port: u16,
    },
}

pub async fn execute_ft_client(args: CommonArgs, cmd: FtClientCommands) {
    let mut client = match get_rpc_client(&args).await {
        Some(c) => c,
        None => fatal("Cannot connect to Fungi daemon. Is it running?"),
    };

    match cmd {
        FtClientCommands::FtpStatus => match client.get_ftp_proxy(Request::new(Empty {})).await {
            Ok(resp) => {
                let proxy = resp.into_inner();
                println!(
                    "FTP Proxy: {}",
                    if proxy.enabled { "enabled" } else { "disabled" }
                );
                println!("Binding: {}:{}", proxy.host, proxy.port);
            }
            Err(e) => fatal_grpc(e),
        },
        FtClientCommands::FtpUpdate {
            enabled,
            host,
            port,
        } => {
            let req = UpdateFtpProxyRequest {
                enabled,
                host,
                port: port as i32,
            };
            match client.update_ftp_proxy(Request::new(req)).await {
                Ok(_) => println!("FTP proxy updated successfully"),
                Err(e) => fatal_grpc(e),
            }
        }
        FtClientCommands::WebdavStatus => {
            match client.get_webdav_proxy(Request::new(Empty {})).await {
                Ok(resp) => {
                    let proxy = resp.into_inner();
                    println!(
                        "WebDAV Proxy: {}",
                        if proxy.enabled { "enabled" } else { "disabled" }
                    );
                    println!("Binding: {}:{}", proxy.host, proxy.port);
                }
                Err(e) => fatal_grpc(e),
            }
        }
        FtClientCommands::WebdavUpdate {
            enabled,
            host,
            port,
        } => {
            let req = UpdateWebdavProxyRequest {
                enabled,
                host,
                port: port as i32,
            };
            match client.update_webdav_proxy(Request::new(req)).await {
                Ok(_) => println!("WebDAV proxy updated successfully"),
                Err(e) => fatal_grpc(e),
            }
        }
        FtClientCommands::List => {
            match client
                .get_all_file_transfer_clients(Request::new(Empty {}))
                .await
            {
                Ok(resp) => {
                    let clients = resp.into_inner().clients;
                    if clients.is_empty() {
                        println!("No file transfer clients");
                    } else {
                        for client in clients {
                            println!(
                                "{} - {} [{}]",
                                client.peer_id,
                                client.name,
                                if client.enabled {
                                    "enabled"
                                } else {
                                    "disabled"
                                }
                            );
                        }
                    }
                }
                Err(e) => fatal_grpc(e),
            }
        }
        FtClientCommands::Add(add_args) => {
            let req = AddFileTransferClientRequest {
                enabled: add_args.enabled,
                name: add_args.name.unwrap_or_default(),
                peer_id: add_args.peer_id,
            };
            match client.add_file_transfer_client(Request::new(req)).await {
                Ok(_) => println!("Client added successfully"),
                Err(e) => fatal_grpc(e),
            }
        }
        FtClientCommands::Remove { peer_id } => {
            let req = RemoveFileTransferClientRequest { peer_id };
            match client.remove_file_transfer_client(Request::new(req)).await {
                Ok(_) => println!("Client removed successfully"),
                Err(e) => fatal_grpc(e),
            }
        }
        FtClientCommands::Enable { peer_id, enabled } => {
            let req = EnableFileTransferClientRequest { peer_id, enabled };
            match client.enable_file_transfer_client(Request::new(req)).await {
                Ok(_) => println!(
                    "Client {} successfully",
                    if enabled { "enabled" } else { "disabled" }
                ),
                Err(e) => fatal_grpc(e),
            }
        }
    }
}
