use clap::Subcommand;
use fungi_daemon_grpc::{
    Request,
    fungi_daemon_grpc::{
        Empty, GetAddressBookPeerRequest, PeerInfo, RemoveAddressBookPeerRequest,
        UpdateAddressBookPeerRequest,
    },
};
use libp2p::PeerId;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::commands::CommonArgs;

use super::{
    client::get_rpc_client,
    shared::{fatal, fatal_grpc, resolve_peer_input},
};

#[derive(Subcommand, Debug, Clone)]
pub enum DeviceCommands {
    /// List devices discovered via mDNS
    Mdns,
    /// List all peers in the address book
    List,
    /// Add a peer to the address book with a required alias
    Add {
        /// Peer ID to add
        peer_id: String,
        /// Human-friendly unique alias
        #[arg(long)]
        alias: String,
    },
    /// Rename an existing peer in the address book
    Rename {
        /// Peer ID or alias to rename
        peer: String,
        /// New human-friendly unique alias
        alias: String,
    },
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

pub async fn execute_device(args: CommonArgs, cmd: DeviceCommands) {
    let mut client = match get_rpc_client(&args).await {
        Some(c) => c,
        None => fatal("Cannot connect to Fungi daemon. Is it running?"),
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
            Err(e) => fatal_grpc(e),
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
                Err(e) => fatal_grpc(e),
            }
        }
        DeviceCommands::Add { peer_id, alias } => {
            let peer_id = match peer_id.parse::<PeerId>() {
                Ok(value) => value,
                Err(error) => fatal(format!("Invalid peer_id: {error}")),
            };

            let existing = match client
                .get_address_book_peer(Request::new(GetAddressBookPeerRequest {
                    peer_id: peer_id.to_string(),
                }))
                .await
            {
                Ok(resp) => resp.into_inner().peer_info,
                Err(error) => fatal_grpc(error),
            };

            let peer_info = match existing {
                Some(mut peer) => {
                    peer.alias = alias;
                    peer
                }
                None => new_minimal_peer_info(peer_id.to_string(), alias),
            };

            match client
                .update_address_book_peer(Request::new(UpdateAddressBookPeerRequest {
                    peer_info: Some(peer_info),
                }))
                .await
            {
                Ok(_) => println!("Peer saved with alias"),
                Err(error) => fatal_grpc(error),
            }
        }
        DeviceCommands::Rename { peer, alias } => {
            let target_peer_id = if let Ok(value) = peer.parse::<PeerId>() {
                value.to_string()
            } else {
                match resolve_peer_input(&args, &peer) {
                    Ok(peer) => peer.peer_id,
                    Err(error) => fatal(error),
                }
            };

            let peer_info = match client
                .get_address_book_peer(Request::new(GetAddressBookPeerRequest {
                    peer_id: target_peer_id,
                }))
                .await
            {
                Ok(resp) => match resp.into_inner().peer_info {
                    Some(mut peer) => {
                        peer.alias = alias;
                        peer
                    }
                    None => fatal("Peer not found in address book"),
                },
                Err(error) => fatal_grpc(error),
            };

            match client
                .update_address_book_peer(Request::new(UpdateAddressBookPeerRequest {
                    peer_info: Some(peer_info),
                }))
                .await
            {
                Ok(_) => println!("Peer alias updated"),
                Err(error) => fatal_grpc(error),
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
                Err(e) => fatal_grpc(e),
            }
        }
        DeviceCommands::Remove { peer_id } => {
            let req = RemoveAddressBookPeerRequest { peer_id };
            match client.remove_address_book_peer(Request::new(req)).await {
                Ok(_) => println!("Peer removed successfully"),
                Err(e) => fatal_grpc(e),
            }
        }
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
