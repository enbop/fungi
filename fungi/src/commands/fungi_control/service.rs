use clap::Subcommand;
use fungi_config::FungiDir;
use fungi_daemon::{
    DiscoveredService, NodeCapabilities, ServiceInstance, load_service_manifest_yaml_file,
};
use fungi_daemon_grpc::{
    Request,
    fungi_daemon_grpc::{
        DeployServiceRequest, DiscoverPeerCapabilitiesRequest, DiscoverPeerServicesRequest,
        GetServiceLogsRequest, ServiceHandleRequest, ServiceInstanceResponse,
    },
};

use crate::commands::CommonArgs;

use super::{
    client::get_rpc_client,
    shared::{fatal, fatal_grpc},
};

#[derive(Subcommand, Debug, Clone)]
pub enum ServiceCommands {
    /// Deploy a service from a YAML manifest file
    Deploy {
        /// Path to a service manifest YAML file
        manifest: String,
    },
    /// Start a deployed service by name
    Start { handle: String },
    /// Inspect a deployed service by name
    Inspect { handle: String },
    /// Get service logs by name
    Logs {
        handle: String,
        #[arg(long)]
        tail: Option<String>,
    },
    /// Stop a deployed service by name
    Stop { handle: String },
    /// Remove a deployed service by name
    Remove { handle: String },
    /// List discoverable services from a remote peer
    Discover { peer_id: String },
    /// Query minimal deployment capabilities from a remote peer
    Capabilities { peer_id: String },
}

pub async fn execute_service(args: CommonArgs, cmd: ServiceCommands) {
    let mut client = match get_rpc_client(&args).await {
        Some(c) => c,
        None => fatal("Cannot connect to Fungi daemon. Is it running?"),
    };

    match cmd {
        ServiceCommands::Deploy { manifest } => {
            let manifest_path = std::path::PathBuf::from(&manifest);
            let loaded = match load_service_manifest_yaml_file(&manifest_path, &args.fungi_dir()) {
                Ok(value) => value,
                Err(error) => fatal(format!("Failed to load manifest: {error}")),
            };

            let manifest_json = match serde_json::to_string(&loaded) {
                Ok(value) => value,
                Err(error) => fatal(format!("Failed to serialize manifest: {error}")),
            };

            let req = DeployServiceRequest { manifest_json };
            match client.deploy_service(Request::new(req)).await {
                Ok(resp) => print_service_instance(resp.into_inner()),
                Err(e) => fatal_grpc(e),
            }
        }
        ServiceCommands::Start { handle } => {
            let req = ServiceHandleRequest { runtime: 0, handle };
            match client.start_service(Request::new(req)).await {
                Ok(_) => println!("Service started"),
                Err(e) => fatal_grpc(e),
            }
        }
        ServiceCommands::Inspect { handle } => {
            let req = ServiceHandleRequest { runtime: 0, handle };
            match client.inspect_service(Request::new(req)).await {
                Ok(resp) => print_service_instance(resp.into_inner()),
                Err(e) => fatal_grpc(e),
            }
        }
        ServiceCommands::Logs { handle, tail } => {
            let req = GetServiceLogsRequest {
                runtime: 0,
                handle,
                tail: tail.unwrap_or_default(),
            };
            match client.get_service_logs(Request::new(req)).await {
                Ok(resp) => {
                    let logs = resp.into_inner();
                    print!("{}", logs.text);
                }
                Err(e) => fatal_grpc(e),
            }
        }
        ServiceCommands::Stop { handle } => {
            let req = ServiceHandleRequest { runtime: 0, handle };
            match client.stop_service(Request::new(req)).await {
                Ok(_) => println!("Service stopped"),
                Err(e) => fatal_grpc(e),
            }
        }
        ServiceCommands::Remove { handle } => {
            let req = ServiceHandleRequest { runtime: 0, handle };
            match client.remove_service(Request::new(req)).await {
                Ok(_) => println!("Service removed"),
                Err(e) => fatal_grpc(e),
            }
        }
        ServiceCommands::Discover { peer_id } => {
            let req = DiscoverPeerServicesRequest { peer_id };
            match client.discover_peer_services(Request::new(req)).await {
                Ok(resp) => print_discovered_services(&resp.into_inner().services_json),
                Err(e) => fatal_grpc(e),
            }
        }
        ServiceCommands::Capabilities { peer_id } => {
            let req = DiscoverPeerCapabilitiesRequest { peer_id };
            match client.discover_peer_capabilities(Request::new(req)).await {
                Ok(resp) => print_node_capabilities(&resp.into_inner().capabilities_json),
                Err(e) => fatal_grpc(e),
            }
        }
    }
}

fn print_service_instance(resp: ServiceInstanceResponse) {
    match serde_json::from_str::<ServiceInstance>(&resp.instance_json) {
        Ok(instance) => match serde_json::to_string_pretty(&instance) {
            Ok(pretty) => println!("{}", pretty),
            Err(error) => fatal(format!("Failed to format service instance: {error}")),
        },
        Err(error) => fatal(format!("Failed to decode service instance: {error}")),
    }
}

fn print_discovered_services(services_json: &str) {
    match serde_json::from_str::<Vec<DiscoveredService>>(services_json) {
        Ok(services) => match serde_json::to_string_pretty(&services) {
            Ok(pretty) => println!("{}", pretty),
            Err(error) => fatal(format!("Failed to format discovered services: {error}")),
        },
        Err(error) => fatal(format!("Failed to decode discovered services: {error}")),
    }
}

fn print_node_capabilities(capabilities_json: &str) {
    match serde_json::from_str::<NodeCapabilities>(capabilities_json) {
        Ok(capabilities) => match serde_json::to_string_pretty(&capabilities) {
            Ok(pretty) => println!("{}", pretty),
            Err(error) => fatal(format!("Failed to format node capabilities: {error}")),
        },
        Err(error) => fatal(format!("Failed to decode node capabilities: {error}")),
    }
}
