use clap::Subcommand;
use fungi_daemon::NodeCapabilities;
use fungi_daemon_grpc::{Request, fungi_daemon_grpc::GetPeerCapabilitySummaryRequest};

use crate::commands::CommonArgs;

use super::{
    client::get_rpc_client,
    shared::{
        PeerInput, PeerTargetArg, clear_current_peer, fatal, fatal_grpc, get_current_peer,
        print_target_peer, resolve_peer_input, resolve_required_peer, set_current_peer,
    },
};

#[derive(Subcommand, Debug, Clone)]
pub enum PeerCommands {
    /// Show the current default peer context
    Current,
    /// Set the current default peer context
    Use {
        /// Peer ID or device name
        peer: PeerInput,
    },
    /// Clear the current default peer context
    Clear,
    /// Query the runtime capability summary of a remote peer
    Capability {
        #[command(flatten)]
        peer: PeerTargetArg,
    },
}

pub async fn execute_peer(args: CommonArgs, cmd: PeerCommands) {
    match cmd {
        PeerCommands::Current => match get_current_peer(&args) {
            Ok(Some(peer)) => print_current_peer(&peer),
            Ok(None) => println!("No current peer selected"),
            Err(error) => fatal(error),
        },
        PeerCommands::Use { peer } => {
            let resolved = match resolve_peer_input(&args, &peer) {
                Ok(peer) => peer,
                Err(error) => fatal(error),
            };
            if let Err(error) = set_current_peer(&args, &resolved) {
                fatal(error)
            }
            print_current_peer(&resolved);
        }
        PeerCommands::Clear => {
            if let Err(error) = clear_current_peer(&args) {
                fatal(error)
            }
            println!("Current peer cleared");
        }
        PeerCommands::Capability { peer } => {
            let resolved = match resolve_required_peer(&args, peer.peer.as_ref()) {
                Ok(peer) => peer,
                Err(error) => fatal(error),
            };
            print_target_peer(&resolved);
            let mut client = match get_rpc_client(&args).await {
                Some(c) => c,
                None => fatal("Cannot connect to Fungi daemon. Is it running?"),
            };
            let req = GetPeerCapabilitySummaryRequest {
                peer_id: resolved.peer_id,
            };
            match client.get_peer_capability_summary(Request::new(req)).await {
                Ok(resp) => {
                    let capability_summary = match serde_json::from_str::<NodeCapabilities>(
                        &resp.into_inner().capability_summary_json,
                    ) {
                        Ok(value) => value,
                        Err(error) => {
                            fatal(format!("Failed to decode peer capability summary: {error}"))
                        }
                    };
                    match serde_json::to_string_pretty(&capability_summary) {
                        Ok(pretty) => println!("{pretty}"),
                        Err(error) => {
                            fatal(format!("Failed to format peer capability summary: {error}"))
                        }
                    }
                }
                Err(error) => fatal_grpc(error),
            }
        }
    }
}

fn print_current_peer(peer: &super::shared::ResolvedPeerTarget) {
    match (&peer.name, &peer.hostname) {
        (Some(name), Some(hostname)) if !name.is_empty() && !hostname.is_empty() => {
            println!("Current peer: {} ({name}) [{hostname}]", peer.peer_id)
        }
        (Some(name), _) if !name.is_empty() => {
            println!("Current peer: {} ({name})", peer.peer_id)
        }
        _ => println!("Current peer: {}", peer.peer_id),
    }
}
