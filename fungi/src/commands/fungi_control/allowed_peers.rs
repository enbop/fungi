use clap::Subcommand;
use fungi_daemon_grpc::{
    Request,
    fungi_daemon_grpc::{AddIncomingAllowedPeerRequest, Empty, RemoveIncomingAllowedPeerRequest},
};

use crate::commands::CommonArgs;

use super::client::get_rpc_client;

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
