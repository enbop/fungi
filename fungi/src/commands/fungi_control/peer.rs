use clap::Subcommand;
use fungi_daemon::NodeCapabilities;
use fungi_daemon_grpc::{
    Request,
    fungi_daemon_grpc::{
        GetPeerCapabilitySummaryRequest, RemotePeerRequest, RemotePullServiceRequest,
        RemoteServiceControlResponse, RemoteServiceNameRequest,
    },
};

use crate::commands::CommonArgs;

use super::{
    client::get_rpc_client,
    service::read_manifest_yaml_file,
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
    /// Administrator operations against a remote peer
    #[command(subcommand)]
    Admin(PeerAdminCommands),
}

#[derive(Subcommand, Debug, Clone)]
pub enum PeerAdminCommands {
    /// Manage service instances on a remote peer
    #[command(subcommand)]
    Service(PeerAdminServiceCommands),
}

#[derive(Subcommand, Debug, Clone)]
pub enum PeerAdminServiceCommands {
    /// List service instances on a remote peer
    List {
        #[command(flatten)]
        peer: PeerTargetArg,
    },
    /// Pull a service manifest to a remote peer
    Pull {
        /// Path to a service manifest YAML file
        manifest: String,
        #[command(flatten)]
        peer: PeerTargetArg,
    },
    /// Start a service instance on a remote peer by name
    Start {
        /// Service name
        name: String,
        #[command(flatten)]
        peer: PeerTargetArg,
    },
    /// Stop a service instance on a remote peer by name
    Stop {
        /// Service name
        name: String,
        #[command(flatten)]
        peer: PeerTargetArg,
    },
    /// Remove a service instance on a remote peer by name
    Remove {
        /// Service name
        name: String,
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
        PeerCommands::Admin(admin_cmd) => match admin_cmd {
            PeerAdminCommands::Service(service_cmd) => match service_cmd {
                PeerAdminServiceCommands::List { peer } => {
                    let peer = match resolve_required_peer(&args, peer.peer.as_ref()) {
                        Ok(peer) => peer,
                        Err(error) => fatal(error),
                    };
                    print_target_peer(&peer);
                    let mut client = match get_rpc_client(&args).await {
                        Some(c) => c,
                        None => fatal("Cannot connect to Fungi daemon. Is it running?"),
                    };
                    let req = RemotePeerRequest {
                        peer_id: peer.peer_id,
                    };
                    match client.remote_list_services(Request::new(req)).await {
                        Ok(resp) => {
                            super::service::print_service_instances(resp.into_inner(), false)
                        }
                        Err(error) => fatal_grpc(error),
                    }
                }
                PeerAdminServiceCommands::Pull { manifest, peer } => {
                    let peer = match resolve_required_peer(&args, peer.peer.as_ref()) {
                        Ok(peer) => peer,
                        Err(error) => fatal(error),
                    };
                    print_target_peer(&peer);
                    let mut client = match get_rpc_client(&args).await {
                        Some(c) => c,
                        None => fatal("Cannot connect to Fungi daemon. Is it running?"),
                    };
                    let created = read_manifest_yaml_file(&manifest);
                    let req = RemotePullServiceRequest {
                        peer_id: peer.peer_id,
                        manifest_yaml: created.manifest_yaml,
                    };
                    match client.remote_pull_service(Request::new(req)).await {
                        Ok(resp) => print_remote_service_result("pulled", resp.into_inner()),
                        Err(error) => fatal_grpc(error),
                    }
                }
                PeerAdminServiceCommands::Start { name, peer } => {
                    let peer = match resolve_required_peer(&args, peer.peer.as_ref()) {
                        Ok(peer) => peer,
                        Err(error) => fatal(error),
                    };
                    print_target_peer(&peer);
                    let mut client = match get_rpc_client(&args).await {
                        Some(c) => c,
                        None => fatal("Cannot connect to Fungi daemon. Is it running?"),
                    };
                    let req = RemoteServiceNameRequest {
                        peer_id: peer.peer_id,
                        name,
                    };
                    match client.remote_start_service(Request::new(req)).await {
                        Ok(resp) => print_remote_service_result("started", resp.into_inner()),
                        Err(error) => fatal_grpc(error),
                    }
                }
                PeerAdminServiceCommands::Stop { name, peer } => {
                    let peer = match resolve_required_peer(&args, peer.peer.as_ref()) {
                        Ok(peer) => peer,
                        Err(error) => fatal(error),
                    };
                    print_target_peer(&peer);
                    let mut client = match get_rpc_client(&args).await {
                        Some(c) => c,
                        None => fatal("Cannot connect to Fungi daemon. Is it running?"),
                    };
                    let req = RemoteServiceNameRequest {
                        peer_id: peer.peer_id,
                        name,
                    };
                    match client.remote_stop_service(Request::new(req)).await {
                        Ok(resp) => print_remote_service_result("stopped", resp.into_inner()),
                        Err(error) => fatal_grpc(error),
                    }
                }
                PeerAdminServiceCommands::Remove { name, peer } => {
                    let peer = match resolve_required_peer(&args, peer.peer.as_ref()) {
                        Ok(peer) => peer,
                        Err(error) => fatal(error),
                    };
                    print_target_peer(&peer);
                    let mut client = match get_rpc_client(&args).await {
                        Some(c) => c,
                        None => fatal("Cannot connect to Fungi daemon. Is it running?"),
                    };
                    let req = RemoteServiceNameRequest {
                        peer_id: peer.peer_id,
                        name,
                    };
                    match client.remote_remove_service(Request::new(req)).await {
                        Ok(resp) => print_remote_service_result("removed", resp.into_inner()),
                        Err(error) => fatal_grpc(error),
                    }
                }
            },
        },
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

fn print_remote_service_result(action: &str, resp: RemoteServiceControlResponse) {
    let service_name = if resp.service_name.trim().is_empty() {
        "<unknown>"
    } else {
        &resp.service_name
    };
    println!("Remote service {action}: {service_name}");
}
