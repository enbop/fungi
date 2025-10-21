use clap::{Args, Subcommand};
use fungi_config::{FungiConfig, FungiDir};
use fungi_daemon_grpc::{
    Request,
    fungi_daemon_grpc::{
        AddFileTransferClientRequest, AddIncomingAllowedPeerRequest, AddTcpForwardingRuleRequest,
        AddTcpListeningRuleRequest, Empty, EnableFileTransferClientRequest,
        GetAddressBookPeerRequest, PeerInfo, RemoveAddressBookPeerRequest,
        RemoveFileTransferClientRequest, RemoveIncomingAllowedPeerRequest,
        RemoveTcpForwardingRuleRequest, RemoveTcpListeningRuleRequest,
        StartFileTransferServiceRequest, UpdateFtpProxyRequest, UpdateWebdavProxyRequest,
        fungi_daemon_client::FungiDaemonClient,
    },
};

use crate::commands::CommonArgs;

fn parse_address(address: &str) -> Result<(String, u16), String> {
    let parts: Vec<&str> = address.rsplitn(2, ':').collect();
    if parts.len() != 2 {
        return Err(format!(
            "Invalid address format: {}. Expected format: host:port",
            address
        ));
    }

    let port = parts[0]
        .parse::<u16>()
        .map_err(|_| format!("Invalid port number: {}", parts[0]))?;
    let host = parts[1].to_string();

    Ok((host, port))
}

#[derive(Subcommand, Debug, Clone)]
pub enum InfoCommands {
    /// Show daemon version
    Version,
    /// Show peer ID of this daemon
    PeerId,
    /// Show hostname of this device
    Hostname,
    /// Show configuration file path
    ConfigPath,
    /// Show RPC address
    RpcAddress,
}

#[derive(Subcommand, Debug, Clone)]
pub enum AllowedPeerCommands {
    /// List peers allowed to initiate incoming connections
    List,
    /// Add a peer to the incoming connection allowlist
    Add {
        /// Peer ID to allow
        peer_id: String,
    },
    /// Remove a peer from the incoming connection allowlist
    Remove {
        /// Peer ID to remove
        peer_id: String,
    },
}

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
    ListClients,
    /// Add a new file transfer client
    AddClient(AddClientArgs),
    /// Remove a file transfer client
    RemoveClient {
        /// Peer ID of the client
        peer_id: String,
    },
    /// Enable or disable a file transfer client
    EnableClient {
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

#[derive(Subcommand, Debug, Clone)]
pub enum TunnelCommands {
    /// Show TCP tunneling configuration
    Config,
    /// Add a TCP forwarding rule
    AddForward {
        /// Local address to bind (format: host:port, e.g., 127.0.0.1:8080)
        local_address: String,
        /// Remote peer ID
        remote_peer_id: String,
        /// Remote port to connect
        remote_port: u16,
    },
    /// Remove a TCP forwarding rule
    RemoveForward {
        /// Local address (format: host:port, e.g., 127.0.0.1:8080)
        local_address: String,
        /// Remote peer ID
        remote_peer_id: String,
        /// Remote port
        remote_port: u16,
    },
    /// Add a TCP listening rule
    AddListen {
        /// Local address to bind (format: host:port, e.g., 127.0.0.1:8080)
        local_address: String,
    },
    /// Remove a TCP listening rule
    RemoveListen {
        /// Local address (format: host:port, e.g., 127.0.0.1:8080)
        local_address: String,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum DeviceCommands {
    /// List devices discovered via mDNS
    Mdns,
    /// List all peers in the address book
    List,
    /// Get information about a specific peer in the address book
    Get {
        /// Peer ID to query
        peer_id: String,
    },
    /// Remove a peer from the address book
    Remove {
        /// Peer ID to remove
        peer_id: String,
    },
}

async fn get_rpc_client(args: &CommonArgs) -> Option<FungiDaemonClient<tonic::transport::Channel>> {
    let fungi_config = FungiConfig::try_read_from_dir(&args.fungi_dir()).ok()?;
    let rpc_addr = format!("http://{}", fungi_config.rpc.listen_address);

    let connect_timeout = std::time::Duration::from_secs(3);
    match tokio::time::timeout(connect_timeout, FungiDaemonClient::connect(rpc_addr)).await {
        Ok(Ok(client)) => Some(client),
        Ok(Err(e)) => {
            eprintln!("Cannot connect to Fungi daemon. Is it running?");
            log::error!("Error connecting to daemon: {}", e);
            None
        }
        Err(_) => {
            eprintln!("Cannot connect to Fungi daemon. Is it running?");
            log::error!(
                "Connection timeout after {} seconds",
                connect_timeout.as_secs()
            );
            None
        }
    }
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
        InfoCommands::PeerId => match client.peer_id(Request::new(Empty {})).await {
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

pub async fn execute_allowed_peer(args: CommonArgs, cmd: AllowedPeerCommands) {
    let mut client = match get_rpc_client(&args).await {
        Some(c) => c,
        None => return,
    };

    match cmd {
        AllowedPeerCommands::List => {
            match client
                .get_incoming_allowed_peers(Request::new(Empty {}))
                .await
            {
                Ok(resp) => {
                    let peers = resp.into_inner().peers;
                    if peers.is_empty() {
                        println!("No allowed peers");
                    } else {
                        for peer in peers {
                            println!("{} - {}", peer.peer_id, peer.alias);
                        }
                    }
                }
                Err(e) => eprintln!("Error: {}", e),
            }
        }
        AllowedPeerCommands::Add { peer_id } => {
            let req = AddIncomingAllowedPeerRequest { peer_id };
            match client.add_incoming_allowed_peer(Request::new(req)).await {
                Ok(_) => println!("Peer added successfully"),
                Err(e) => eprintln!("Error: {}", e),
            }
        }
        AllowedPeerCommands::Remove { peer_id } => {
            let req = RemoveIncomingAllowedPeerRequest { peer_id };
            match client.remove_incoming_allowed_peer(Request::new(req)).await {
                Ok(_) => println!("Peer removed successfully"),
                Err(e) => eprintln!("Error: {}", e),
            }
        }
    }
}

pub async fn execute_ft_service(args: CommonArgs, cmd: FtServiceCommands) {
    let mut client = match get_rpc_client(&args).await {
        Some(c) => c,
        None => return,
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

                    if enabled {
                        if let Ok(dir_resp) = client
                            .get_file_transfer_service_root_dir(Request::new(Empty {}))
                            .await
                        {
                            println!("Root directory: {}", dir_resp.into_inner().root_dir);
                        }
                    }
                }
                Err(e) => eprintln!("Error: {}", e),
            }
        }
        FtServiceCommands::Start { root_dir } => {
            let req = StartFileTransferServiceRequest { root_dir };
            match client.start_file_transfer_service(Request::new(req)).await {
                Ok(_) => println!("File transfer service started"),
                Err(e) => eprintln!("Error: {}", e),
            }
        }
        FtServiceCommands::Stop => {
            match client
                .stop_file_transfer_service(Request::new(Empty {}))
                .await
            {
                Ok(_) => println!("File transfer service stopped"),
                Err(e) => eprintln!("Error: {}", e),
            }
        }
    }
}

pub async fn execute_ft_client(args: CommonArgs, cmd: FtClientCommands) {
    let mut client = match get_rpc_client(&args).await {
        Some(c) => c,
        None => return,
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
            Err(e) => eprintln!("Error: {}", e),
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
                Err(e) => eprintln!("Error: {}", e),
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
                Err(e) => eprintln!("Error: {}", e),
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
                Err(e) => eprintln!("Error: {}", e),
            }
        }
        FtClientCommands::ListClients => {
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
                Err(e) => eprintln!("Error: {}", e),
            }
        }
        FtClientCommands::AddClient(add_args) => {
            let req = AddFileTransferClientRequest {
                enabled: add_args.enabled,
                name: add_args.name.unwrap_or_default(),
                peer_id: add_args.peer_id,
            };
            match client.add_file_transfer_client(Request::new(req)).await {
                Ok(_) => println!("Client added successfully"),
                Err(e) => eprintln!("Error: {}", e),
            }
        }
        FtClientCommands::RemoveClient { peer_id } => {
            let req = RemoveFileTransferClientRequest { peer_id };
            match client.remove_file_transfer_client(Request::new(req)).await {
                Ok(_) => println!("Client removed successfully"),
                Err(e) => eprintln!("Error: {}", e),
            }
        }
        FtClientCommands::EnableClient { peer_id, enabled } => {
            let req = EnableFileTransferClientRequest { peer_id, enabled };
            match client.enable_file_transfer_client(Request::new(req)).await {
                Ok(_) => println!(
                    "Client {} successfully",
                    if enabled { "enabled" } else { "disabled" }
                ),
                Err(e) => eprintln!("Error: {}", e),
            }
        }
    }
}

pub async fn execute_tunnel(args: CommonArgs, cmd: TunnelCommands) {
    let mut client = match get_rpc_client(&args).await {
        Some(c) => c,
        None => return,
    };

    match cmd {
        TunnelCommands::Config => {
            match client
                .get_tcp_tunneling_config(Request::new(Empty {}))
                .await
            {
                Ok(resp) => {
                    let config = resp.into_inner();
                    println!(
                        "Forwarding: {}",
                        if config.forwarding_enabled {
                            "enabled"
                        } else {
                            "disabled"
                        }
                    );
                    println!(
                        "Listening: {}",
                        if config.listening_enabled {
                            "enabled"
                        } else {
                            "disabled"
                        }
                    );

                    if !config.forwarding_rules.is_empty() {
                        println!("\nForwarding Rules:");
                        for rule in config.forwarding_rules {
                            println!(
                                "  {}:{} -> {}:{}",
                                rule.local_host,
                                rule.local_port,
                                rule.remote_peer_id,
                                rule.remote_port
                            );
                        }
                    }

                    if !config.listening_rules.is_empty() {
                        println!("\nListening Rules:");
                        for rule in config.listening_rules {
                            println!("  {}:{}", rule.host, rule.port);
                        }
                    }
                }
                Err(e) => eprintln!("Error: {}", e),
            }
        }
        TunnelCommands::AddForward {
            local_address,
            remote_peer_id,
            remote_port,
        } => {
            let (local_host, local_port) = match parse_address(&local_address) {
                Ok(addr) => addr,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    return;
                }
            };

            let req = AddTcpForwardingRuleRequest {
                local_host,
                local_port: local_port as i32,
                peer_id: remote_peer_id,
                remote_port: remote_port as i32,
            };
            match client.add_tcp_forwarding_rule(Request::new(req)).await {
                Ok(resp) => println!("Forwarding rule added: {}", resp.into_inner().rule_id),
                Err(e) => eprintln!("Error: {}", e),
            }
        }
        TunnelCommands::RemoveForward {
            local_address,
            remote_peer_id,
            remote_port,
        } => {
            let (local_host, local_port) = match parse_address(&local_address) {
                Ok(addr) => addr,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    return;
                }
            };

            let req = RemoveTcpForwardingRuleRequest {
                local_host,
                local_port: local_port as i32,
                peer_id: remote_peer_id,
                remote_port: remote_port as i32,
            };
            match client.remove_tcp_forwarding_rule(Request::new(req)).await {
                Ok(_) => println!("Forwarding rule removed successfully"),
                Err(e) => eprintln!("Error: {}", e),
            }
        }
        TunnelCommands::AddListen { local_address } => {
            let (local_host, local_port) = match parse_address(&local_address) {
                Ok(addr) => addr,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    return;
                }
            };

            let req = AddTcpListeningRuleRequest {
                local_host,
                local_port: local_port as i32,
                allowed_peers: vec![],
            };
            match client.add_tcp_listening_rule(Request::new(req)).await {
                Ok(resp) => println!("Listening rule added: {}", resp.into_inner().rule_id),
                Err(e) => eprintln!("Error: {}", e),
            }
        }
        TunnelCommands::RemoveListen { local_address } => {
            let (local_host, local_port) = match parse_address(&local_address) {
                Ok(addr) => addr,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    return;
                }
            };

            let req = RemoveTcpListeningRuleRequest {
                local_host,
                local_port: local_port as i32,
            };
            match client.remove_tcp_listening_rule(Request::new(req)).await {
                Ok(_) => println!("Listening rule removed successfully"),
                Err(e) => eprintln!("Error: {}", e),
            }
        }
    }
}

pub async fn execute_device(args: CommonArgs, cmd: DeviceCommands) {
    let mut client = match get_rpc_client(&args).await {
        Some(c) => c,
        None => return,
    };

    match cmd {
        DeviceCommands::Mdns => match client.list_mdns_devices(Request::new(Empty {})).await {
            Ok(resp) => {
                let peers = resp.into_inner().peers;
                if peers.is_empty() {
                    println!("No devices discovered");
                } else {
                    for peer in peers {
                        print_peer_info(&peer);
                    }
                }
            }
            Err(e) => eprintln!("Error: {}", e),
        },
        DeviceCommands::List => {
            match client.list_address_book_peers(Request::new(Empty {})).await {
                Ok(resp) => {
                    let peers = resp.into_inner().peers;
                    if peers.is_empty() {
                        println!("No peers in address book");
                    } else {
                        for peer in peers {
                            print_peer_info(&peer);
                        }
                    }
                }
                Err(e) => eprintln!("Error: {}", e),
            }
        }
        DeviceCommands::Get { peer_id } => {
            let req = GetAddressBookPeerRequest { peer_id };
            match client.get_address_book_peer(Request::new(req)).await {
                Ok(resp) => {
                    if let Some(peer) = resp.into_inner().peer_info {
                        print_peer_info_detailed(&peer);
                    } else {
                        println!("Peer not found");
                    }
                }
                Err(e) => eprintln!("Error: {}", e),
            }
        }
        DeviceCommands::Remove { peer_id } => {
            let req = RemoveAddressBookPeerRequest { peer_id };
            match client.remove_address_book_peer(Request::new(req)).await {
                Ok(_) => println!("Peer removed successfully"),
                Err(e) => eprintln!("Error: {}", e),
            }
        }
    }
}

fn print_peer_info(peer: &PeerInfo) {
    println!("{} - {} ({})", peer.peer_id, peer.alias, peer.hostname);
}

fn print_peer_info_detailed(peer: &PeerInfo) {
    println!("Peer ID: {}", peer.peer_id);
    println!("Alias: {}", peer.alias);
    println!("Hostname: {}", peer.hostname);
    println!("OS: {}", peer.os);
    println!("Version: {}", peer.version);
    if !peer.public_ip.is_empty() {
        println!("Public IP: {}", peer.public_ip);
    }
    if !peer.private_ips.is_empty() {
        println!("Private IPs: {}", peer.private_ips.join(", "));
    }
}
