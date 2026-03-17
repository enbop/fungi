use clap::Subcommand;
use fungi_daemon_grpc::{
    Request,
    fungi_daemon_grpc::{AddIncomingAllowedPeerRequest, Empty, RemoveIncomingAllowedPeerRequest},
};

use crate::commands::CommonArgs;

use super::{
    client::get_rpc_client,
    shared::{fatal, fatal_grpc, require_named_peer_by_id},
};

#[derive(Subcommand, Debug, Clone)]
pub enum AllowedPeerCommands {
    /// List peers allowed to initiate incoming connections
    List,
    /// Add a peer to the incoming connection allowlist
    Add {
        /// Peer ID or alias of an already named peer
        peer: String,
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
        AllowedPeerCommands::Add { peer } => {
            let resolved = match require_named_peer_by_id(&args, &peer) {
                Ok(peer) => peer,
                Err(error) => fatal(error),
            };
            let req = AddIncomingAllowedPeerRequest {
                peer_id: resolved.peer_id,
            };
            match client.add_incoming_allowed_peer(Request::new(req)).await {
                Ok(_) => println!("Peer added successfully"),
                Err(e) => fatal_grpc(e),
            }
        }
        AllowedPeerCommands::Remove { peer } => {
            let peer_id = match require_named_peer_by_id(&args, &peer) {
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
