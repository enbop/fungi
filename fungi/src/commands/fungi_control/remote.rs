use clap::Subcommand;
use fungi_config::FungiDir;
use fungi_daemon::load_service_manifest_yaml_file;
use fungi_daemon_grpc::{
    Request,
    fungi_daemon_grpc::{
        DiscoverPeerCapabilitiesRequest, DiscoverPeerServicesRequest, RemoteDeployServiceRequest,
        RemoteServiceControlResponse, RemoteServiceHandleRequest,
    },
};

use crate::commands::CommonArgs;

use super::{
    client::get_rpc_client,
    service::{print_discovered_services, print_node_capabilities},
    shared::{fatal, fatal_grpc},
};

#[derive(Subcommand, Debug, Clone)]
pub enum RemoteCommands {
    /// Query remote node capabilities
    Capabilities {
        /// Peer ID to query
        peer_id: String,
    },
    /// Query or control remote services
    #[command(subcommand)]
    Service(RemoteServiceCommands),
}

#[derive(Subcommand, Debug, Clone)]
pub enum RemoteServiceCommands {
    /// List discoverable services from a remote peer
    Discover {
        /// Peer ID to query
        peer_id: String,
    },
    /// Deploy a service manifest to a remote peer
    Deploy {
        /// Peer ID to control
        peer_id: String,
        /// Path to a service manifest YAML file
        manifest: String,
    },
    /// Start a deployed service on a remote peer by name
    Start {
        /// Peer ID to control
        peer_id: String,
        /// Service name
        handle: String,
    },
    /// Stop a deployed service on a remote peer by name
    Stop {
        /// Peer ID to control
        peer_id: String,
        /// Service name
        handle: String,
    },
    /// Remove a deployed service on a remote peer by name
    Remove {
        /// Peer ID to control
        peer_id: String,
        /// Service name
        handle: String,
    },
}

pub async fn execute_remote(args: CommonArgs, cmd: RemoteCommands) {
    let mut client = match get_rpc_client(&args).await {
        Some(c) => c,
        None => fatal("Cannot connect to Fungi daemon. Is it running?"),
    };

    match cmd {
        RemoteCommands::Capabilities { peer_id } => {
            let req = DiscoverPeerCapabilitiesRequest { peer_id };
            match client.discover_peer_capabilities(Request::new(req)).await {
                Ok(resp) => print_node_capabilities(&resp.into_inner().capabilities_json),
                Err(e) => fatal_grpc(e),
            }
        }
        RemoteCommands::Service(service_cmd) => match service_cmd {
            RemoteServiceCommands::Discover { peer_id } => {
                let req = DiscoverPeerServicesRequest { peer_id };
                match client.discover_peer_services(Request::new(req)).await {
                    Ok(resp) => print_discovered_services(&resp.into_inner().services_json),
                    Err(e) => fatal_grpc(e),
                }
            }
            RemoteServiceCommands::Deploy { peer_id, manifest } => {
                let manifest_path = std::path::PathBuf::from(&manifest);
                let loaded = match load_service_manifest_yaml_file(&manifest_path, &args.fungi_dir()) {
                    Ok(value) => value,
                    Err(error) => fatal(format!("Failed to load manifest: {error}")),
                };

                let manifest_json = match serde_json::to_string(&loaded) {
                    Ok(value) => value,
                    Err(error) => fatal(format!("Failed to serialize manifest: {error}")),
                };

                let req = RemoteDeployServiceRequest {
                    peer_id,
                    manifest_json,
                };
                match client.remote_deploy_service(Request::new(req)).await {
                    Ok(resp) => print_remote_service_result("deployed", resp.into_inner()),
                    Err(e) => fatal_grpc(e),
                }
            }
            RemoteServiceCommands::Start { peer_id, handle } => {
                let req = RemoteServiceHandleRequest { peer_id, handle };
                match client.remote_start_service(Request::new(req)).await {
                    Ok(resp) => print_remote_service_result("started", resp.into_inner()),
                    Err(e) => fatal_grpc(e),
                }
            }
            RemoteServiceCommands::Stop { peer_id, handle } => {
                let req = RemoteServiceHandleRequest { peer_id, handle };
                match client.remote_stop_service(Request::new(req)).await {
                    Ok(resp) => print_remote_service_result("stopped", resp.into_inner()),
                    Err(e) => fatal_grpc(e),
                }
            }
            RemoteServiceCommands::Remove { peer_id, handle } => {
                let req = RemoteServiceHandleRequest { peer_id, handle };
                match client.remote_remove_service(Request::new(req)).await {
                    Ok(resp) => print_remote_service_result("removed", resp.into_inner()),
                    Err(e) => fatal_grpc(e),
                }
            }
        },
    }
}

fn print_remote_service_result(action: &str, resp: RemoteServiceControlResponse) {
    let service_name = if resp.service_name.trim().is_empty() {
        "<unknown>"
    } else {
        &resp.service_name
    };
    println!("Remote service {}: {}", action, service_name);
}