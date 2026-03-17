use fungi_daemon_grpc::{
    Request,
    fungi_daemon_grpc::{PingPeerRequest, ping_peer_event},
};

use crate::commands::CommonArgs;

use super::{
    client::get_rpc_client,
    shared::{
        fatal, fatal_grpc, shorten_peer_id, simplify_multiaddr_peer_ids,
        summarize_ping_error_message,
    },
};

pub async fn execute_ping(args: CommonArgs, peer_id: String, interval_ms: u32, verbose: bool) {
    let mut client = match get_rpc_client(&args).await {
        Some(c) => c,
        None => fatal("Cannot connect to Fungi daemon. Is it running?"),
    };

    let req = PingPeerRequest {
        peer_id: peer_id.clone(),
        interval_ms,
    };

    let response = match client.ping_peer(Request::new(req)).await {
        Ok(resp) => resp,
        Err(e) => fatal_grpc(e),
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
            "{:<6} {:<6} {:<8} {:<5} {:<10} ADDR/MSG",
            "TICK", "CONN", "DIR", "RLY", "RTT"
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
                            summarize_ping_error_message(&error.message, verbose)
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
