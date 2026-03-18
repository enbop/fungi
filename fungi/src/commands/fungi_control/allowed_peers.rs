use clap::Subcommand;
use fungi_daemon_grpc::{
    Request,
    fungi_daemon_grpc::{
        AddIncomingAllowedPeerRequest, Empty, GetAddressBookPeerRequest, PeerInfo,
        RemoveIncomingAllowedPeerRequest, RuntimeConfigResponse, UpdateAddressBookPeerRequest,
    },
};
use libp2p::PeerId;
use std::io::{self, Write};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::commands::CommonArgs;

use super::{
    client::get_rpc_client,
    shared::{fatal, fatal_grpc, resolve_peer_value},
};

#[derive(Subcommand, Debug, Clone)]
pub enum AllowedPeerCommands {
    /// List peers allowed to initiate incoming connections
    List,
    /// Add a peer to the incoming connection allowlist
    Add {
        /// Peer ID or alias. For an unnamed peer ID, pass --alias to save it.
        peer: String,
        /// Alias to add or update in the address book
        #[arg(long)]
        alias: Option<String>,
    },
    /// Remove a peer from the incoming connection allowlist
    Remove {
        /// Peer ID or alias to remove
        peer: String,
    },
}

pub async fn execute_allowed_peer(args: CommonArgs, cmd: AllowedPeerCommands) {
    let mut client = match get_rpc_client(&args).await {
        Some(c) => c,
        None => fatal("Cannot connect to Fungi daemon. Is it running?"),
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
                Err(e) => fatal_grpc(e),
            }
        }
        AllowedPeerCommands::Add { peer, alias } => {
            let resolved = resolve_allowed_peer_for_add(&args, &mut client, &peer, alias).await;
            let runtime_config = get_runtime_config(&mut client).await;

            if !confirm_allowed_peer_add(&resolved, &runtime_config) {
                println!("Aborted. No changes were made.");
                return;
            }

            if resolved.address_book_needs_update {
                let alias = resolved
                    .alias
                    .clone()
                    .unwrap_or_else(|| fatal("Missing alias for address book update"));
                let existing = get_address_book_peer(&mut client, &resolved.peer_id).await;
                upsert_address_book_peer(&mut client, existing, &resolved.peer_id, alias).await;
            }

            let req = AddIncomingAllowedPeerRequest {
                peer_id: resolved.peer_id.clone(),
            };
            match client.add_incoming_allowed_peer(Request::new(req)).await {
                Ok(_) => {
                    if resolved.address_book_needs_update {
                        let alias = resolved.alias.as_deref().unwrap_or("<unnamed>");
                        println!("Address book updated: {} -> {}", resolved.peer_id, alias);
                    }
                    println!("Peer added successfully");
                    print_allowed_peer_warning(&resolved);
                }
                Err(e) => fatal_grpc(e),
            }
        }
        AllowedPeerCommands::Remove { peer } => {
            let peer_id = match resolve_peer_value(&args, &peer) {
                Ok(peer) => peer.peer_id,
                Err(_) => peer,
            };
            let req = RemoveIncomingAllowedPeerRequest { peer_id };
            match client.remove_incoming_allowed_peer(Request::new(req)).await {
                Ok(_) => println!("Peer removed successfully"),
                Err(e) => fatal_grpc(e),
            }
        }
    }
}

#[derive(Debug, Clone)]
struct AllowedPeerTarget {
    peer_id: String,
    alias: Option<String>,
    address_book_needs_update: bool,
}

async fn resolve_allowed_peer_for_add(
    args: &CommonArgs,
    client: &mut fungi_daemon_grpc::fungi_daemon_grpc::fungi_daemon_client::FungiDaemonClient<
        tonic::transport::Channel,
    >,
    peer: &str,
    alias: Option<String>,
) -> AllowedPeerTarget {
    let requested_alias = alias
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    if let Ok(peer_id) = peer.parse::<PeerId>() {
        let peer_id = peer_id.to_string();
        let existing = get_address_book_peer(client, &peer_id).await;
        let address_book_needs_update = requested_alias.is_some();

        let alias = match requested_alias {
            Some(alias) => Some(alias),
            None => existing
                .as_ref()
                .map(|info| info.alias.trim().to_string())
                .filter(|value| !value.is_empty())
                .or_else(|| {
                    fatal(format!(
                        "Peer {} is not named yet. Re-run with `--alias <name>` to add it to the address book and allowlist in one step.",
                        peer_id
                    ))
                }),
        };

        return AllowedPeerTarget {
            peer_id,
            alias,
            address_book_needs_update,
        };
    }

    let resolved = match resolve_peer_value(args, peer) {
        Ok(peer) => peer,
        Err(error) => fatal(error),
    };

    if let Some(alias) = requested_alias {
        return AllowedPeerTarget {
            peer_id: resolved.peer_id,
            alias: Some(alias),
            address_book_needs_update: true,
        };
    }

    AllowedPeerTarget {
        peer_id: resolved.peer_id,
        alias: resolved.alias,
        address_book_needs_update: false,
    }
}

async fn get_runtime_config(
    client: &mut fungi_daemon_grpc::fungi_daemon_grpc::fungi_daemon_client::FungiDaemonClient<
        tonic::transport::Channel,
    >,
) -> RuntimeConfigResponse {
    match client.get_runtime_config(Request::new(Empty {})).await {
        Ok(resp) => resp.into_inner(),
        Err(error) => fatal_grpc(error),
    }
}

async fn get_address_book_peer(
    client: &mut fungi_daemon_grpc::fungi_daemon_grpc::fungi_daemon_client::FungiDaemonClient<
        tonic::transport::Channel,
    >,
    peer_id: &str,
) -> Option<PeerInfo> {
    match client
        .get_address_book_peer(Request::new(GetAddressBookPeerRequest {
            peer_id: peer_id.to_string(),
        }))
        .await
    {
        Ok(resp) => resp.into_inner().peer_info,
        Err(error) => fatal_grpc(error),
    }
}

async fn upsert_address_book_peer(
    client: &mut fungi_daemon_grpc::fungi_daemon_grpc::fungi_daemon_client::FungiDaemonClient<
        tonic::transport::Channel,
    >,
    existing: Option<PeerInfo>,
    peer_id: &str,
    alias: String,
) {
    let peer_info = match existing {
        Some(mut peer_info) => {
            peer_info.alias = alias;
            peer_info
        }
        None => new_minimal_peer_info(peer_id.to_string(), alias),
    };

    match client
        .update_address_book_peer(Request::new(UpdateAddressBookPeerRequest {
            peer_info: Some(peer_info),
        }))
        .await
    {
        Ok(_) => {}
        Err(error) => fatal_grpc(error),
    }
}

fn new_minimal_peer_info(peer_id: String, alias: String) -> PeerInfo {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    PeerInfo {
        peer_id,
        alias,
        hostname: String::new(),
        os: "Unknown".to_string(),
        public_ip: String::new(),
        private_ips: Vec::new(),
        created_at: now,
        last_connected: now,
        version: String::new(),
    }
}

fn print_allowed_peer_warning(peer: &AllowedPeerTarget) {
    let display_name = peer.alias.as_deref().unwrap_or(&peer.peer_id);

    println!();
    println!("================ IMPORTANT SECURITY NOTICE ================");
    println!(
        "{} is now allowed to access host paths configured via `security allow-path`.",
        display_name
    );
    println!("{} can also manage services on this device.", display_name);
    println!("Peer ID: {}", peer.peer_id);
    println!("===========================================================");
}

fn confirm_allowed_peer_add(peer: &AllowedPeerTarget, config: &RuntimeConfigResponse) -> bool {
    let display_name = peer.alias.as_deref().unwrap_or(&peer.peer_id);

    println!();
    println!("================ SECURITY CONFIRMATION ================");
    println!("You are about to trust this peer for incoming access:");
    println!("  Peer: {}", display_name);
    println!("  Peer ID: {}", peer.peer_id);
    if peer.address_book_needs_update {
        println!("  Address book: will be added or updated");
    }
    println!();
    println!("This peer will be able to:");
    println!("  1. Access these allowed host paths:");
    print_string_list(&config.allowed_host_paths, "    - none configured");
    println!("  2. Manage services on this device");
    println!("  3. Use these explicitly allowed host ports:");
    print_i32_list(&config.allowed_ports, "    - none configured");
    println!("  4. Use these allowed host port ranges:");
    print_port_ranges(&config.allowed_port_ranges);
    println!("=======================================================");

    prompt_yes_no("Proceed? [Y/n]: ")
}

fn prompt_yes_no(prompt: &str) -> bool {
    print!("{prompt}");
    if io::stdout().flush().is_err() {
        fatal("Failed to flush confirmation prompt")
    }

    let mut input = String::new();
    match io::stdin().read_line(&mut input) {
        Ok(_) => match input.trim().to_ascii_lowercase().as_str() {
            "" | "y" | "yes" => true,
            "n" | "no" => false,
            other => fatal(format!(
                "Invalid confirmation response: {other}. Expected Y, y, N, n, yes, no, or Enter."
            )),
        },
        Err(error) => fatal(format!("Failed to read confirmation response: {error}")),
    }
}

fn print_string_list(items: &[String], empty_message: &str) {
    if items.is_empty() {
        println!("{empty_message}");
        return;
    }

    for item in items {
        println!("    - {item}");
    }
}

fn print_i32_list(items: &[i32], empty_message: &str) {
    if items.is_empty() {
        println!("{empty_message}");
        return;
    }

    for item in items {
        println!("    - {item}");
    }
}

fn print_port_ranges(ranges: &[fungi_daemon_grpc::fungi_daemon_grpc::RuntimeAllowedPortRange]) {
    if ranges.is_empty() {
        println!("    - none configured");
        return;
    }

    for range in ranges {
        println!("    - {}-{}", range.start, range.end);
    }
}
