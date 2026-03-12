use clap::Subcommand;
use fungi_daemon::{
    DiscoveredService, EnabledRemoteService, NodeCapabilities, RuntimeKind, ServiceInstance,
    ServicePortProtocol,
};
use fungi_daemon_grpc::{
    Request,
    fungi_daemon_grpc::{
        DeployServiceRequest, Empty, GetServiceLogsRequest, ListServicesResponse,
        ServiceInstanceResponse, ServiceNameRequest,
    },
};
use serde::Serialize;

use crate::commands::CommonArgs;

use super::{
    client::get_rpc_client,
    shared::{fatal, fatal_grpc},
};

#[derive(Subcommand, Debug, Clone)]
pub enum ServiceCommands {
    /// List deployed services on the local node, including stopped ones
    List,
    /// Deploy a service from a YAML manifest file
    Deploy {
        /// Path to a service manifest YAML file
        manifest: String,
    },
    /// Start a deployed service by name
    Start { name: String },
    /// Inspect a deployed service by name
    Inspect { name: String },
    /// Get service logs by name
    Logs {
        name: String,
        #[arg(long)]
        tail: Option<String>,
    },
    /// Stop a deployed service by name
    Stop { name: String },
    /// Remove a deployed service by name
    Remove { name: String },
}

pub async fn execute_service(args: CommonArgs, cmd: ServiceCommands) {
    let mut client = match get_rpc_client(&args).await {
        Some(c) => c,
        None => fatal("Cannot connect to Fungi daemon. Is it running?"),
    };

    match cmd {
        ServiceCommands::List => match client.list_services(Request::new(Empty {})).await {
            Ok(resp) => print_service_instances(resp.into_inner()),
            Err(e) => fatal_grpc(e),
        },
        ServiceCommands::Deploy { manifest } => {
            let (manifest_yaml, manifest_base_dir) = read_manifest_yaml_file(&manifest);
            let req = DeployServiceRequest {
                manifest_yaml,
                manifest_base_dir,
            };
            match client.deploy_service(Request::new(req)).await {
                Ok(resp) => print_service_instance(resp.into_inner()),
                Err(e) => fatal_grpc(e),
            }
        }
        ServiceCommands::Start { name } => {
            let req = ServiceNameRequest { runtime: 0, name };
            match client.start_service(Request::new(req)).await {
                Ok(_) => println!("Service started"),
                Err(e) => fatal_grpc(e),
            }
        }
        ServiceCommands::Inspect { name } => {
            let req = ServiceNameRequest { runtime: 0, name };
            match client.inspect_service(Request::new(req)).await {
                Ok(resp) => print_service_instance(resp.into_inner()),
                Err(e) => fatal_grpc(e),
            }
        }
        ServiceCommands::Logs { name, tail } => {
            let req = GetServiceLogsRequest {
                runtime: 0,
                name,
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
        ServiceCommands::Stop { name } => {
            let req = ServiceNameRequest { runtime: 0, name };
            match client.stop_service(Request::new(req)).await {
                Ok(_) => println!("Service stopped"),
                Err(e) => fatal_grpc(e),
            }
        }
        ServiceCommands::Remove { name } => {
            let req = ServiceNameRequest { runtime: 0, name };
            match client.remove_service(Request::new(req)).await {
                Ok(_) => println!("Service removed"),
                Err(e) => fatal_grpc(e),
            }
        }
    }
}

pub(crate) fn read_manifest_yaml_file(path: &str) -> (String, String) {
    let manifest_path = std::path::PathBuf::from(path);
    let absolute_manifest_path = match std::fs::canonicalize(&manifest_path) {
        Ok(path) => path,
        Err(error) => fatal(format!("Failed to resolve manifest path: {error}")),
    };
    let manifest_yaml = match std::fs::read_to_string(&absolute_manifest_path) {
        Ok(value) => value,
        Err(error) => fatal(format!("Failed to read manifest: {error}")),
    };
    let manifest_base_dir = absolute_manifest_path
        .parent()
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_default();
    (manifest_yaml, manifest_base_dir)
}

pub(crate) fn print_service_instance(resp: ServiceInstanceResponse) {
    match serde_json::from_str::<ServiceInstance>(&resp.instance_json) {
        Ok(instance) => {
            match serde_json::to_string_pretty(&LocalServiceInspectView::from(instance)) {
                Ok(pretty) => println!("{}", pretty),
                Err(error) => fatal(format!("Failed to format service instance: {error}")),
            }
        }
        Err(error) => fatal(format!("Failed to decode service instance: {error}")),
    }
}

pub(crate) fn print_service_instances(resp: ListServicesResponse) {
    match serde_json::from_str::<Vec<ServiceInstance>>(&resp.services_json) {
        Ok(services) => {
            let views = services
                .into_iter()
                .map(LocalServiceListEntry::from)
                .collect::<Vec<_>>();
            match serde_json::to_string_pretty(&views) {
                Ok(pretty) => println!("{}", pretty),
                Err(error) => fatal(format!("Failed to format service list: {error}")),
            }
        }
        Err(error) => fatal(format!("Failed to decode service list: {error}")),
    }
}

pub(crate) fn print_discovered_services(services_json: &str) {
    match serde_json::from_str::<Vec<DiscoveredService>>(services_json) {
        Ok(services) => match serde_json::to_string_pretty(&services) {
            Ok(pretty) => println!("{}", pretty),
            Err(error) => fatal(format!("Failed to format discovered services: {error}")),
        },
        Err(error) => fatal(format!("Failed to decode discovered services: {error}")),
    }
}

pub(crate) fn print_node_capabilities(capabilities_json: &str) {
    match serde_json::from_str::<NodeCapabilities>(capabilities_json) {
        Ok(capabilities) => match serde_json::to_string_pretty(&capabilities) {
            Ok(pretty) => println!("{}", pretty),
            Err(error) => fatal(format!("Failed to format node capabilities: {error}")),
        },
        Err(error) => fatal(format!("Failed to decode node capabilities: {error}")),
    }
}

pub(crate) fn print_enabled_remote_service(service_json: &str) {
    match serde_json::from_str::<EnabledRemoteService>(service_json) {
        Ok(service) => match serde_json::to_string_pretty(&service) {
            Ok(pretty) => println!("{}", pretty),
            Err(error) => fatal(format!("Failed to format enabled remote service: {error}")),
        },
        Err(error) => fatal(format!("Failed to decode enabled remote service: {error}")),
    }
}

pub(crate) fn print_enabled_remote_services(services_json: &str) {
    match serde_json::from_str::<Vec<EnabledRemoteService>>(services_json) {
        Ok(services) => match serde_json::to_string_pretty(&services) {
            Ok(pretty) => println!("{}", pretty),
            Err(error) => fatal(format!("Failed to format enabled remote services: {error}")),
        },
        Err(error) => fatal(format!("Failed to decode enabled remote services: {error}")),
    }
}

#[derive(Debug, Serialize)]
struct RemoteServiceListEntry {
    service_name: String,
    runtime: RuntimeKind,
    state: String,
    running: bool,
    discoverable: bool,
    service_id: Option<String>,
    local_forwarded: bool,
    available_endpoints: Vec<RemoteServiceEndpointView>,
    local_forwarded_endpoints: Vec<RemoteForwardedEndpointView>,
}

#[derive(Debug, Serialize)]
struct RemoteServiceEndpointView {
    name: String,
    protocol: String,
    service_port: u16,
}

#[derive(Debug, Serialize)]
struct RemoteForwardedEndpointView {
    name: String,
    local_host: String,
    local_port: u16,
    protocol: String,
}

#[derive(Debug, Serialize)]
struct LocalServiceListEntry {
    service_name: String,
    runtime: RuntimeKind,
    state: String,
    running: bool,
    local_endpoints: Vec<LocalServiceEndpointView>,
}

#[derive(Debug, Serialize)]
struct LocalServiceInspectView {
    id: String,
    name: String,
    runtime: RuntimeKind,
    source: String,
    labels: std::collections::BTreeMap<String, String>,
    state: String,
    running: bool,
    local_endpoints: Vec<LocalServiceEndpointView>,
    exposed_endpoints: Vec<LocalExposedEndpointView>,
}

#[derive(Debug, Serialize)]
struct LocalServiceEndpointView {
    name: Option<String>,
    protocol: String,
    local_host: String,
    local_port: u16,
    service_port: u16,
}

#[derive(Debug, Serialize)]
struct LocalExposedEndpointView {
    name: String,
    protocol: String,
    local_host: String,
    local_port: u16,
    service_port: u16,
}

impl From<ServiceInstance> for LocalServiceListEntry {
    fn from(instance: ServiceInstance) -> Self {
        let local_endpoints = local_endpoint_views(&instance);
        Self {
            service_name: instance.name,
            runtime: instance.runtime,
            state: instance.status.state,
            running: instance.status.running,
            local_endpoints,
        }
    }
}

impl From<ServiceInstance> for LocalServiceInspectView {
    fn from(instance: ServiceInstance) -> Self {
        let local_endpoints = local_endpoint_views(&instance);
        Self {
            id: instance.id,
            name: instance.name,
            runtime: instance.runtime,
            source: instance.source,
            labels: instance.labels,
            state: instance.status.state,
            running: instance.status.running,
            local_endpoints,
            exposed_endpoints: instance
                .exposed_endpoints
                .into_iter()
                .map(|endpoint| LocalExposedEndpointView {
                    name: endpoint.name,
                    protocol: endpoint.protocol,
                    local_host: "127.0.0.1".to_string(),
                    local_port: endpoint.host_port,
                    service_port: endpoint.service_port,
                })
                .collect(),
        }
    }
}

fn local_endpoint_views(instance: &ServiceInstance) -> Vec<LocalServiceEndpointView> {
    instance
        .ports
        .iter()
        .map(|port| LocalServiceEndpointView {
            name: port.name.clone(),
            protocol: local_port_protocol_name(port.protocol).to_string(),
            local_host: "127.0.0.1".to_string(),
            local_port: port.host_port,
            service_port: port.service_port,
        })
        .collect()
}

fn local_port_protocol_name(protocol: ServicePortProtocol) -> &'static str {
    match protocol {
        ServicePortProtocol::Tcp => "tcp",
        ServicePortProtocol::Udp => "udp",
    }
}

pub(crate) fn print_remote_service_list(
    services_json: &str,
    discovered_json: &str,
    enabled_json: &str,
) {
    let services = match serde_json::from_str::<Vec<ServiceInstance>>(services_json) {
        Ok(value) => value,
        Err(error) => fatal(format!("Failed to decode remote service list: {error}")),
    };
    let discovered = match serde_json::from_str::<Vec<DiscoveredService>>(discovered_json) {
        Ok(value) => value,
        Err(error) => fatal(format!("Failed to decode discovered services: {error}")),
    };
    let enabled = match serde_json::from_str::<Vec<EnabledRemoteService>>(enabled_json) {
        Ok(value) => value,
        Err(error) => fatal(format!(
            "Failed to decode forwarded remote services: {error}"
        )),
    };

    let discovered_by_name = discovered
        .into_iter()
        .map(|service| (service.service_name.clone(), service))
        .collect::<std::collections::BTreeMap<_, _>>();
    let enabled_by_name = enabled
        .into_iter()
        .map(|service| (service.service_name.clone(), service))
        .collect::<std::collections::BTreeMap<_, _>>();

    let rows = services
        .into_iter()
        .map(|service| {
            let discovered = discovered_by_name.get(&service.name);
            let enabled = enabled_by_name.get(&service.name);

            RemoteServiceListEntry {
                service_name: service.name,
                runtime: service.runtime,
                state: service.status.state,
                running: service.status.running,
                discoverable: discovered.is_some(),
                service_id: discovered.map(|value| value.service_id.clone()),
                local_forwarded: enabled.is_some(),
                available_endpoints: discovered
                    .map(|value| {
                        value
                            .endpoints
                            .iter()
                            .map(|endpoint| RemoteServiceEndpointView {
                                name: endpoint.name.clone(),
                                protocol: endpoint.protocol.clone(),
                                service_port: endpoint.service_port,
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default(),
                local_forwarded_endpoints: enabled
                    .map(|value| {
                        value
                            .endpoints
                            .iter()
                            .map(|endpoint| RemoteForwardedEndpointView {
                                name: endpoint.name.clone(),
                                local_host: endpoint.local_host.clone(),
                                local_port: endpoint.local_port,
                                protocol: endpoint.protocol.clone(),
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default(),
            }
        })
        .collect::<Vec<_>>();

    match serde_json::to_string_pretty(&rows) {
        Ok(pretty) => println!("{}", pretty),
        Err(error) => fatal(format!("Failed to format remote service list: {error}")),
    }
}
