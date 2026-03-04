use clap::Subcommand;
use fungi_daemon_grpc::{
    Request,
    fungi_daemon_grpc::{
        AddTcpForwardingRuleRequest, AddTcpListeningRuleRequest, Empty,
        RemoveTcpForwardingRuleRequest, RemoveTcpListeningRuleRequest,
    },
};

use crate::commands::CommonArgs;

use super::{client::get_rpc_client, shared::parse_address};

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
