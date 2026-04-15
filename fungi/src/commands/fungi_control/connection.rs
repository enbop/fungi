use clap::Subcommand;
use fungi_daemon_grpc::{
    Request,
    fungi_daemon_grpc::{Empty, ListActiveStreamsRequest, ListConnectionsRequest},
};

use crate::commands::CommonArgs;

use super::{
    client::get_rpc_client,
    shared::{
        connection_id_sort_key, fatal, fatal_grpc, shorten_peer_id, simplify_multiaddr_peer_ids,
    },
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
    /// Show observed external address candidates for this daemon
    AddrCandidates {
        /// Show detailed output
        #[arg(short, long, default_value_t = false)]
        verbose: bool,
    },
    /// Show runtime relay endpoint status tracked by the daemon
    RelayStatus {
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
        ConnectionCommands::AddrCandidates { verbose } => {
            execute_addr_candidates(args, verbose).await
        }
        ConnectionCommands::RelayStatus { verbose } => execute_relay_status(args, verbose).await,
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
        None => fatal("Cannot connect to Fungi daemon. Is it running?"),
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
                "{:<22} {:<6} {:<8} {:<5} {:<12} {:<7} ADDR",
                "PEER", "CONN", "DIR", "RLY", "LAST_PING", "STREAMS"
            );

            let mut direct_streams_total = 0u64;
            let mut relay_streams_total = 0u64;
            let mut matched_protocol_streams_total = 0u64;

            for conn in connections {
                let ping = if conn.last_ping_unix_ms == 0 {
                    "n/a".to_string()
                } else if verbose {
                    format!("{}ms @ {}", conn.last_rtt_ms, conn.last_ping_unix_ms)
                } else {
                    format!("{}ms", conn.last_rtt_ms)
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
        Err(e) => fatal_grpc(e),
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
        None => fatal("Cannot connect to Fungi daemon. Is it running?"),
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
                "{:<8} {:<22} {:<6} {:<14} PROTOCOL",
                "STREAM", "PEER", "CONN", "OPENED_AT"
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
        Err(e) => fatal_grpc(e),
    }
}

async fn execute_addr_candidates(args: CommonArgs, verbose: bool) {
    let mut client = match get_rpc_client(&args).await {
        Some(c) => c,
        None => fatal("Cannot connect to Fungi daemon. Is it running?"),
    };

    match client
        .list_external_address_candidates(Request::new(Empty {}))
        .await
    {
        Ok(resp) => {
            let candidates = resp.into_inner().candidates;
            if candidates.is_empty() {
                println!("No external address candidates recorded");
                return;
            }

            println!("External address candidates");
            println!(
                "{:<8} {:<8} {:<8} {:<14} ADDR",
                "TRANSP", "OBS", "CNFRM", "LAST_SEEN"
            );

            for candidate in candidates {
                let addr_display = if verbose {
                    candidate.address.clone()
                } else {
                    simplify_multiaddr_peer_ids(&candidate.address)
                };

                println!(
                    "{:<8} {:<8} {:<8} {:<14} {}",
                    candidate.transport,
                    candidate.observation_count,
                    if candidate.confirmed_at_unix_ms == 0 {
                        "no"
                    } else {
                        "yes"
                    },
                    candidate.last_observed_at_unix_ms,
                    addr_display,
                );

                if verbose {
                    println!(
                        "  first_seen={} confirmed_at={} expired_at={} sources={}",
                        candidate.first_observed_at_unix_ms,
                        candidate.confirmed_at_unix_ms,
                        candidate.expired_at_unix_ms,
                        candidate.sources.join(",")
                    );
                }
            }
        }
        Err(e) => fatal_grpc(e),
    }
}

async fn execute_relay_status(args: CommonArgs, verbose: bool) {
    let mut client = match get_rpc_client(&args).await {
        Some(c) => c,
        None => fatal("Cannot connect to Fungi daemon. Is it running?"),
    };

    match client
        .list_relay_endpoint_statuses(Request::new(Empty {}))
        .await
    {
        Ok(resp) => {
            let statuses = resp.into_inner().statuses;
            if statuses.is_empty() {
                println!("No relay endpoints tracked");
                return;
            }

            println!("Relay endpoint status");
            println!(
                "{:<8} {:<8} {:<8} {:<16} ADDR",
                "TRANSP", "LSTNR", "TASK", "LAST_ACTION"
            );

            for status in statuses {
                let addr_display = if verbose {
                    status.relay_addr.clone()
                } else {
                    simplify_multiaddr_peer_ids(&status.relay_addr)
                };
                let last_action = if status.last_management_action.is_empty() {
                    "-"
                } else {
                    &status.last_management_action
                };

                println!(
                    "{:<8} {:<8} {:<8} {:<16} {}",
                    status.transport,
                    if status.listener_registered {
                        "yes"
                    } else {
                        "no"
                    },
                    if status.task_running { "yes" } else { "no" },
                    last_action,
                    addr_display,
                );

                if verbose {
                    println!(
                        "  peer_id={} last_seen={} last_missing={} reservation_at={} closed_at={} error={}",
                        if status.relay_peer_id.is_empty() {
                            "-"
                        } else {
                            &status.relay_peer_id
                        },
                        status.last_listener_seen_at_unix_ms,
                        status.last_listener_missing_at_unix_ms,
                        status.last_reservation_accepted_at_unix_ms,
                        status.last_direct_connection_closed_at_unix_ms,
                        if status.last_error.is_empty() {
                            "-"
                        } else {
                            &status.last_error
                        }
                    );
                }
            }
        }
        Err(e) => fatal_grpc(e),
    }
}
