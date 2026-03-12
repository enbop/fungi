use clap::Subcommand;
use fungi_daemon_grpc::{
    Request,
    fungi_daemon_grpc::{
        DisableRemoteServiceRequest, DiscoverPeerCapabilitiesRequest, DiscoverPeerServicesRequest,
        EnableRemoteServiceRequest, ListEnabledRemoteServicesRequest, RemoteDeployServiceRequest,
        RemotePeerRequest, RemoteServiceControlResponse, RemoteServiceNameRequest,
    },
};

use crate::commands::CommonArgs;

use super::{
    client::get_rpc_client,
    service::{
        print_discovered_services, print_enabled_remote_service, print_enabled_remote_services,
        print_node_capabilities, print_remote_service_list, read_manifest_yaml_file,
    },
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
    /// List remote services with discoverability and local forwarding status
    List {
        /// Peer ID to query
        peer_id: String,
    },
    /// List discoverable services from a remote peer
    Discover {
        /// Peer ID to query
        peer_id: String,
    },
    /// Create local port forwards for a discovered remote service
    Forward {
        /// Peer ID to control
        peer_id: String,
        /// Stable remote service ID
        service_id: String,
    },
    /// Remove local port forwards for a remote service
    Unforward {
        /// Peer ID to control
        peer_id: String,
        /// Stable remote service ID
        service_id: String,
    },
    /// List remote services currently forwarded to local ports
    Forwarded {
        /// Optional peer ID filter
        peer_id: Option<String>,
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
        name: String,
    },
    /// Stop a deployed service on a remote peer by name
    Stop {
        /// Peer ID to control
        peer_id: String,
        /// Service name
        name: String,
    },
    /// Remove a deployed service on a remote peer by name
    Remove {
        /// Peer ID to control
        peer_id: String,
        /// Service name
        name: String,
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
            RemoteServiceCommands::List { peer_id } => {
                let list_req = RemotePeerRequest {
                    peer_id: peer_id.clone(),
                };
                let discover_req = DiscoverPeerServicesRequest {
                    peer_id: peer_id.clone(),
                };
                let forwarded_req = ListEnabledRemoteServicesRequest { peer_id };

                let list_resp = match client.remote_list_services(Request::new(list_req)).await {
                    Ok(resp) => resp.into_inner(),
                    Err(e) => fatal_grpc(e),
                };
                let discover_resp = match client
                    .discover_peer_services(Request::new(discover_req))
                    .await
                {
                    Ok(resp) => resp.into_inner(),
                    Err(e) => fatal_grpc(e),
                };
                let forwarded_resp = match client
                    .list_enabled_remote_services(Request::new(forwarded_req))
                    .await
                {
                    Ok(resp) => resp.into_inner(),
                    Err(e) => fatal_grpc(e),
                };

                print_remote_service_list(
                    &list_resp.services_json,
                    &discover_resp.services_json,
                    &forwarded_resp.enabled_services_json,
                )
            }
            RemoteServiceCommands::Discover { peer_id } => {
                let req = DiscoverPeerServicesRequest { peer_id };
                match client.discover_peer_services(Request::new(req)).await {
                    Ok(resp) => print_discovered_services(&resp.into_inner().services_json),
                    Err(e) => fatal_grpc(e),
                }
            }
            RemoteServiceCommands::Forward {
                peer_id,
                service_id,
            } => {
                let req = EnableRemoteServiceRequest {
                    peer_id,
                    service_id,
                };
                match client.enable_remote_service(Request::new(req)).await {
                    Ok(resp) => {
                        print_enabled_remote_service(&resp.into_inner().enabled_service_json)
                    }
                    Err(e) => fatal_grpc(e),
                }
            }
            RemoteServiceCommands::Unforward {
                peer_id,
                service_id,
            } => {
                let req = DisableRemoteServiceRequest {
                    peer_id,
                    service_id,
                };
                match client.disable_remote_service(Request::new(req)).await {
                    Ok(_) => println!("Remote service local forwarding removed"),
                    Err(e) => fatal_grpc(e),
                }
            }
            RemoteServiceCommands::Forwarded { peer_id } => {
                let req = ListEnabledRemoteServicesRequest {
                    peer_id: peer_id.unwrap_or_default(),
                };
                match client.list_enabled_remote_services(Request::new(req)).await {
                    Ok(resp) => {
                        print_enabled_remote_services(&resp.into_inner().enabled_services_json)
                    }
                    Err(e) => fatal_grpc(e),
                }
            }
            RemoteServiceCommands::Deploy { peer_id, manifest } => {
                let (manifest_yaml, _manifest_base_dir) = read_manifest_yaml_file(&manifest);

                let req = RemoteDeployServiceRequest {
                    peer_id,
                    manifest_yaml,
                };
                match client.remote_deploy_service(Request::new(req)).await {
                    Ok(resp) => print_remote_service_result("deployed", resp.into_inner()),
                    Err(e) => fatal_grpc(e),
                }
            }
            RemoteServiceCommands::Start { peer_id, name } => {
                let req = RemoteServiceNameRequest { peer_id, name };
                match client.remote_start_service(Request::new(req)).await {
                    Ok(resp) => print_remote_service_result("started", resp.into_inner()),
                    Err(e) => fatal_grpc(e),
                }
            }
            RemoteServiceCommands::Stop { peer_id, name } => {
                let req = RemoteServiceNameRequest { peer_id, name };
                match client.remote_stop_service(Request::new(req)).await {
                    Ok(resp) => print_remote_service_result("stopped", resp.into_inner()),
                    Err(e) => fatal_grpc(e),
                }
            }
            RemoteServiceCommands::Remove { peer_id, name } => {
                let req = RemoteServiceNameRequest { peer_id, name };
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
