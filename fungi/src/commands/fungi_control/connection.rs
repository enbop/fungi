use clap::Subcommand;
use fungi_daemon_grpc::{
    Request,
    fungi_daemon_grpc::{ListActiveStreamsRequest, ListConnectionsRequest},
};

use crate::commands::CommonArgs;

use super::{
    client::get_rpc_client,
    shared::{connection_id_sort_key, shorten_peer_id, simplify_multiaddr_peer_ids},
};

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

async fn execute_connections(
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

async fn execute_connection_streams(
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
