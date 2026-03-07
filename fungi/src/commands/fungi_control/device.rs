use clap::Subcommand;
use fungi_daemon_grpc::{
    Request,
    fungi_daemon_grpc::{Empty, GetAddressBookPeerRequest, PeerInfo, RemoveAddressBookPeerRequest},
};

use crate::commands::CommonArgs;

use super::{client::get_rpc_client, shared::{fatal, fatal_grpc}};

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
