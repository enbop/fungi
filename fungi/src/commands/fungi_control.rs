use clap::{Args, Subcommand};
use fungi_config::{FungiConfig, FungiDir};
use fungi_daemon_grpc::{
    Request,
    fungi_daemon_grpc::{
        AddFileTransferClientRequest, AddIncomingAllowedPeerRequest, AddTcpForwardingRuleRequest,
        AddTcpListeningRuleRequest, Empty, EnableFileTransferClientRequest,
        GetAddressBookPeerRequest, ListActiveStreamsRequest, ListConnectionsRequest, PeerInfo,
        PingPeerRequest, RemoveAddressBookPeerRequest, RemoveFileTransferClientRequest,
        RemoveIncomingAllowedPeerRequest, RemoveTcpForwardingRuleRequest,
        RemoveTcpListeningRuleRequest, StartFileTransferServiceRequest, UpdateFtpProxyRequest,
        UpdateWebdavProxyRequest, fungi_daemon_client::FungiDaemonClient, ping_peer_event,
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
    Id,
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

#[derive(Subcommand, Debug, Clone)]
pub enum TunnelCommands {
    /// Show TCP tunneling configuration
    Show,
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

#[derive(Subcommand, Debug, Clone)]
pub enum ConnectionCommands {
    /// Overview of active connections and per-protocol stream counts
    Overview {
        /// Optional peer ID filter
        #[arg(long)]
        peer_id: Option<String>,
        /// Optional protocol filter (e.g. /fungi/tunnel/8080/1.0.0)
        #[arg(long)]
        protocol_name: Option<String>,
        /// Show detailed output
        #[arg(short, long, default_value_t = false)]
        verbose: bool,
    },
    /// List active streams with optional filters
    Streams {
        /// Optional peer ID filter
        #[arg(long)]
        peer_id: Option<String>,
        /// Optional protocol filter (e.g. /fungi/tunnel/8080/1.0.0)
        #[arg(long)]
        protocol_name: Option<String>,
        /// Show detailed output
        #[arg(short, long, default_value_t = false)]
        verbose: bool,
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

fn shorten_peer_id(peer_id: &str) -> String {
    if peer_id.len() <= 18 {
        return peer_id.to_string();
    }
    format!("{}****{}", &peer_id[..8], &peer_id[peer_id.len() - 6..])
}

fn simplify_multiaddr_peer_ids(addr: &str) -> String {
    let mut parts: Vec<String> = addr
        .split('/')
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .collect();

    let mut i = 0;
    while i + 1 < parts.len() {
        if parts[i] == "p2p" {
            parts[i + 1] = shorten_peer_id(&parts[i + 1]);
            i += 2;
        } else {
            i += 1;
        }
    }

    format!("/{}", parts.join("/"))
}

fn connection_id_sort_key(connection_id: &str) -> u64 {
    if let Ok(value) = connection_id.parse::<u64>() {
        return value;
    }

    let digits: String = connection_id
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect();
    digits.parse::<u64>().unwrap_or(u64::MAX)
}

pub async fn execute_ping(args: CommonArgs, peer_id: String, interval_ms: u32, verbose: bool) {
    let mut client = match get_rpc_client(&args).await {
        Some(c) => c,
        None => return,
    };

    let req = PingPeerRequest {
        peer_id: peer_id.clone(),
        interval_ms,
    };

    let response = match client.ping_peer(Request::new(req)).await {
        Ok(resp) => resp,
        Err(e) => {
            eprintln!("Error: {}", e);
            return;
        }
    };

    let mut stream = response.into_inner();
    println!(
        "Ping stream peer={} interval={}ms (Ctrl+C to stop)",
        if verbose {
            peer_id.clone()
        } else {
            shorten_peer_id(&peer_id)
        },
        interval_ms
    );

    if !verbose {
        println!(
            "{:<6} {:<6} {:<8} {:<5} {:<10} {}",
            "TICK", "CONN", "DIR", "RLY", "RTT", "ADDR/MSG"
        );
    }

    while let Ok(Some(event)) = stream.message().await {
        match event.event {
            Some(ping_peer_event::Event::Connecting(_)) => {
                if verbose {
                    println!("[tick={}] connecting...", event.tick_seq);
                } else {
                    println!(
                        "{:<6} {:<6} {:<8} {:<5} {:<10} connecting",
                        event.tick_seq, "-", "-", "-", "-"
                    );
                }
            }
            Some(ping_peer_event::Event::Connected(_)) => {
                if verbose {
                    println!("[tick={}] connected", event.tick_seq);
                } else {
                    println!(
                        "{:<6} {:<6} {:<8} {:<5} {:<10} connected",
                        event.tick_seq, "-", "-", "-", "-"
                    );
                }
            }
            Some(ping_peer_event::Event::Idle(_)) => {
                if verbose {
                    println!("[tick={}] no active connections", event.tick_seq);
                } else {
                    println!(
                        "{:<6} {:<6} {:<8} {:<5} {:<10} no active connections",
                        event.tick_seq, "-", "-", "-", "-"
                    );
                }
            }
            Some(ping_peer_event::Event::Result(result)) => {
                if verbose {
                    println!(
                        "[tick={}] conn={} dir={} addr={} rtt={}ms",
                        event.tick_seq,
                        result.connection_id,
                        result.direction,
                        result.remote_addr,
                        result.rtt_ms
                    );
                } else {
                    let relay = if result.remote_addr.contains("/p2p-circuit") {
                        "yes"
                    } else {
                        "no"
                    };
                    println!(
                        "{:<6} {:<6} {:<8} {:<5} {:<10} {}",
                        event.tick_seq,
                        result.connection_id,
                        result.direction,
                        relay,
                        format!("{}ms", result.rtt_ms),
                        simplify_multiaddr_peer_ids(&result.remote_addr)
                    );
                }
            }
            Some(ping_peer_event::Event::Error(error)) => {
                if verbose {
                    if error.connection_id.is_empty() {
                        println!("[tick={}] error={}", event.tick_seq, error.message);
                    } else {
                        println!(
                            "[tick={}] conn={} dir={} addr={} error={}",
                            event.tick_seq,
                            error.connection_id,
                            error.direction,
                            error.remote_addr,
                            error.message
                        );
                    }
                } else {
                    let relay = if error.remote_addr.contains("/p2p-circuit") {
                        "yes"
                    } else {
                        "no"
                    };
                    println!(
                        "{:<6} {:<6} {:<8} {:<5} {:<10} {}",
                        event.tick_seq,
                        if error.connection_id.is_empty() {
                            "-"
                        } else {
                            &error.connection_id
                        },
                        error.direction,
                        relay,
                        "error",
                        if error.remote_addr.is_empty() {
                            error.message
                        } else {
                            format!(
                                "{} | {}",
                                simplify_multiaddr_peer_ids(&error.remote_addr),
                                error.message
                            )
                        }
                    );
                }
            }
            _ => {
                if verbose {
                    println!("[tick={}] unknown event", event.tick_seq);
                } else {
                    println!(
                        "{:<6} {:<6} {:<8} {:<5} {:<10} unknown event",
                        event.tick_seq, "-", "-", "-", "-"
                    );
                }
            }
        }
    }
}

pub async fn execute_connection(args: CommonArgs, cmd: ConnectionCommands) {
    match cmd {
        ConnectionCommands::Overview {
            peer_id,
            protocol_name,
            verbose,
        } => execute_connections(args, peer_id, protocol_name, verbose).await,
        ConnectionCommands::Streams {
            peer_id,
            protocol_name,
            verbose,
        } => execute_connection_streams(args, peer_id, protocol_name, verbose).await,
    }
}

pub async fn execute_connections(
    args: CommonArgs,
    peer_id: Option<String>,
    protocol_name: Option<String>,
    verbose: bool,
) {
    let mut client = match get_rpc_client(&args).await {
        Some(c) => c,
        None => return,
    };

    let req = ListConnectionsRequest {
        peer_id: peer_id.clone().unwrap_or_default(),
    };

    match client.list_connections(Request::new(req)).await {
        Ok(resp) => {
            let mut connections = resp.into_inner().connections;
            if connections.is_empty() {
                if let Some(pid) = peer_id {
                    println!("No active connections for peer {}", pid);
                } else {
                    println!("No active connections");
                }
                return;
            }

            connections.sort_by(|a, b| {
                connection_id_sort_key(&a.connection_id)
                    .cmp(&connection_id_sort_key(&b.connection_id))
                    .then(a.peer_id.cmp(&b.peer_id))
            });

            println!(
                "Connection overview {}{}",
                peer_id
                    .as_ref()
                    .map(|p| {
                        if verbose {
                            format!("(peer={})", p)
                        } else {
                            format!("(peer={})", shorten_peer_id(p))
                        }
                    })
                    .unwrap_or_else(|| "(all peers)".to_string()),
                protocol_name
                    .as_ref()
                    .map(|p| format!(" (protocol={})", p))
                    .unwrap_or_default()
            );
            println!(
                "{:<22} {:<6} {:<8} {:<5} {:<12} {:<7} {}",
                "PEER", "CONN", "DIR", "RLY", "LAST_PING", "STREAMS", "ADDR"
            );

            let mut direct_streams_total = 0u64;
            let mut relay_streams_total = 0u64;
            let mut matched_protocol_streams_total = 0u64;

            for conn in connections {
                let ping = if conn.last_ping_unix_ms == 0 {
                    "n/a".to_string()
                } else {
                    if verbose {
                        format!("{}ms @ {}", conn.last_rtt_ms, conn.last_ping_unix_ms)
                    } else {
                        format!("{}ms", conn.last_rtt_ms)
                    }
                };

                let stream_count_for_view = match &protocol_name {
                    Some(protocol_filter) => conn
                        .active_streams_by_protocol
                        .iter()
                        .find(|p| p.protocol_name == *protocol_filter)
                        .map(|p| p.stream_count)
                        .unwrap_or(0),
                    None => conn.active_streams_total,
                };

                if stream_count_for_view == 0 && protocol_name.is_some() {
                    continue;
                }

                if conn.is_relay {
                    relay_streams_total += stream_count_for_view;
                } else {
                    direct_streams_total += stream_count_for_view;
                }
                matched_protocol_streams_total += stream_count_for_view;

                let peer_display = if verbose {
                    conn.peer_id
                } else {
                    shorten_peer_id(&conn.peer_id)
                };
                let addr_display = if verbose {
                    conn.remote_addr
                } else {
                    simplify_multiaddr_peer_ids(&conn.remote_addr)
                };
                println!(
                    "{:<22} {:<6} {:<8} {:<5} {:<12} {:<7} {}",
                    peer_display,
                    conn.connection_id,
                    conn.direction,
                    if conn.is_relay { "yes" } else { "no" },
                    ping,
                    stream_count_for_view,
                    addr_display,
                );

                if verbose {
                    for protocol in conn.active_streams_by_protocol {
                        println!(
                            "  - protocol={} streams={}",
                            protocol.protocol_name, protocol.stream_count
                        );
                    }
                }
            }

            if protocol_name.is_some() {
                println!(
                    "Summary: total_streams={} direct_streams={} relay_streams={}",
                    matched_protocol_streams_total, direct_streams_total, relay_streams_total
                );
            }
        }
        Err(e) => eprintln!("Error: {}", e),
    }
}

pub async fn execute_connection_streams(
    args: CommonArgs,
    peer_id: Option<String>,
    protocol_name: Option<String>,
    verbose: bool,
) {
    let mut client = match get_rpc_client(&args).await {
        Some(c) => c,
        None => return,
    };

    let req = ListActiveStreamsRequest {
        peer_id: peer_id.clone().unwrap_or_default(),
        protocol_name: protocol_name.clone().unwrap_or_default(),
    };

    match client.list_active_streams(Request::new(req)).await {
        Ok(resp) => {
            let mut streams = resp.into_inner().streams;
            if streams.is_empty() {
                println!("No active streams");
                return;
            }

            streams.sort_by(|a, b| {
                a.stream_id
                    .cmp(&b.stream_id)
                    .then(a.peer_id.cmp(&b.peer_id))
                    .then(
                        connection_id_sort_key(&a.connection_id)
                            .cmp(&connection_id_sort_key(&b.connection_id)),
                    )
            });

            println!(
                "Active streams {}{}",
                peer_id
                    .as_ref()
                    .map(|p| {
                        if verbose {
                            format!("(peer={})", p)
                        } else {
                            format!("(peer={})", shorten_peer_id(p))
                        }
                    })
                    .unwrap_or_else(|| "(all peers)".to_string()),
                protocol_name
                    .as_ref()
                    .map(|p| format!(" (protocol={})", p))
                    .unwrap_or_default()
            );
            println!(
                "{:<8} {:<22} {:<6} {:<14} {}",
                "STREAM", "PEER", "CONN", "OPENED_AT", "PROTOCOL"
            );

            for stream in streams {
                let opened_at = if stream.opened_at_unix_ms == 0 {
                    "n/a".to_string()
                } else {
                    stream.opened_at_unix_ms.to_string()
                };

                let peer_display = if verbose {
                    stream.peer_id
                } else {
                    shorten_peer_id(&stream.peer_id)
                };

                println!(
                    "{:<8} {:<22} {:<6} {:<14} {}",
                    stream.stream_id,
                    peer_display,
                    stream.connection_id,
                    opened_at,
                    stream.protocol_name,
                );
            }
        }
        Err(e) => eprintln!("Error: {}", e),
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

                    if enabled
                        && let Ok(dir_resp) = client
                            .get_file_transfer_service_root_dir(Request::new(Empty {}))
                            .await
                    {
                        println!("Root directory: {}", dir_resp.into_inner().root_dir);
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
                Err(e) => eprintln!("Error: {}", e),
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
                Err(e) => eprintln!("Error: {}", e),
            }
        }
        FtClientCommands::Remove { peer_id } => {
            let req = RemoveFileTransferClientRequest { peer_id };
            match client.remove_file_transfer_client(Request::new(req)).await {
                Ok(_) => println!("Client removed successfully"),
                Err(e) => eprintln!("Error: {}", e),
            }
        }
        FtClientCommands::Enable { peer_id, enabled } => {
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
        TunnelCommands::Show => {
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
