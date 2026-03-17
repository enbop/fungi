use clap::Subcommand;
use fungi_daemon::{RuntimeKind, ServiceInstance, ServicePortProtocol};
use fungi_daemon_grpc::{
    Request,
    fungi_daemon_grpc::{
        Empty, GetServiceLogsRequest, ListServicesResponse, PullServiceRequest,
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
    /// List local services on this node, including stopped ones
    List {
        /// Show detailed output
        #[arg(short, long, default_value_t = false)]
        verbose: bool,
    },
    /// Pull a service manifest onto the local node
    Pull {
        /// Path to a service manifest YAML file
        manifest: String,
    },
    /// Start a local service by name
    Start { name: String },
    /// Inspect a local service by name
    Inspect {
        name: String,
        /// Show detailed output
        #[arg(short, long, default_value_t = false)]
        verbose: bool,
    },
    /// Get local service logs by name
    Logs {
        name: String,
        #[arg(long)]
        tail: Option<String>,
    },
    /// Stop a local service by name
    Stop { name: String },
    /// Remove a local service by name
    Remove { name: String },
}

pub async fn execute_service(args: CommonArgs, cmd: ServiceCommands) {
    let mut client = match get_rpc_client(&args).await {
        Some(c) => c,
        None => fatal("Cannot connect to Fungi daemon. Is it running?"),
    };

    match cmd {
        ServiceCommands::List { verbose } => {
            match client.list_services(Request::new(Empty {})).await {
                Ok(resp) => print_service_instances(resp.into_inner(), verbose),
                Err(e) => fatal_grpc(e),
            }
        }
        ServiceCommands::Pull { manifest } => {
            let (manifest_yaml, manifest_base_dir) = read_manifest_yaml_file(&manifest);
            let req = PullServiceRequest {
                manifest_yaml,
                manifest_base_dir,
            };
            match client.pull_service(Request::new(req)).await {
                Ok(resp) => print_service_instance(resp.into_inner(), false),
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
        ServiceCommands::Inspect { name, verbose } => {
            let req = ServiceNameRequest { runtime: 0, name };
            match client.inspect_service(Request::new(req)).await {
                Ok(resp) => print_service_instance(resp.into_inner(), verbose),
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

pub(crate) fn print_service_instance(resp: ServiceInstanceResponse, verbose: bool) {
    match serde_json::from_str::<ServiceInstance>(&resp.instance_json) {
        Ok(instance) => {
            let pretty = if verbose {
                serde_json::to_string_pretty(&LocalServiceInspectVerboseView::from(instance))
            } else {
                serde_json::to_string_pretty(&LocalServiceInspectView::from(instance))
            };
            match pretty {
                Ok(pretty) => println!("{}", pretty),
                Err(error) => fatal(format!("Failed to format service instance: {error}")),
            }
        }
        Err(error) => fatal(format!("Failed to decode service instance: {error}")),
    }
}

pub(crate) fn print_service_instances(resp: ListServicesResponse, verbose: bool) {
    match serde_json::from_str::<Vec<ServiceInstance>>(&resp.services_json) {
        Ok(services) => {
            let pretty = if verbose {
                let views = services
                    .into_iter()
                    .map(LocalServiceListVerboseEntry::from)
                    .collect::<Vec<_>>();
                serde_json::to_string_pretty(&views)
            } else {
                let views = services
                    .into_iter()
                    .map(LocalServiceListEntry::from)
                    .collect::<Vec<_>>();
                serde_json::to_string_pretty(&views)
            };
            match pretty {
                Ok(pretty) => println!("{}", pretty),
                Err(error) => fatal(format!("Failed to format service list: {error}")),
            }
        }
        Err(error) => fatal(format!("Failed to decode service list: {error}")),
    }
}

#[derive(Debug, Serialize)]
struct LocalServiceListEntry {
    service_name: String,
    state: String,
    running: bool,
    local_endpoints: Vec<LocalServiceEndpointView>,
}

#[derive(Debug, Serialize)]
struct LocalServiceListVerboseEntry {
    service_name: String,
    runtime: RuntimeKind,
    state: String,
    running: bool,
    local_endpoints: Vec<LocalServiceEndpointVerboseView>,
}

#[derive(Debug, Serialize)]
struct LocalServiceInspectView {
    name: String,
    state: String,
    running: bool,
    local_endpoints: Vec<LocalServiceEndpointView>,
    published_endpoints: Vec<PublishedEndpointView>,
}

#[derive(Debug, Serialize)]
struct LocalServiceInspectVerboseView {
    id: String,
    name: String,
    runtime: RuntimeKind,
    source: String,
    labels: std::collections::BTreeMap<String, String>,
    state: String,
    running: bool,
    local_endpoints: Vec<LocalServiceEndpointVerboseView>,
    published_endpoints: Vec<PublishedEndpointVerboseView>,
}

#[derive(Debug, Serialize)]
struct LocalServiceEndpointView {
    name: Option<String>,
    local_address: String,
}

#[derive(Debug, Serialize)]
struct LocalServiceEndpointVerboseView {
    name: Option<String>,
    protocol: String,
    local_host: String,
    local_port: u16,
    service_port: u16,
}

#[derive(Debug, Serialize)]
struct PublishedEndpointView {
    name: String,
    local_address: String,
}

#[derive(Debug, Serialize)]
struct PublishedEndpointVerboseView {
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
            state: instance.status.state,
            running: instance.status.running,
            local_endpoints,
        }
    }
}

impl From<ServiceInstance> for LocalServiceListVerboseEntry {
    fn from(instance: ServiceInstance) -> Self {
        let local_endpoints = local_endpoint_verbose_views(&instance);
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
            name: instance.name,
            state: instance.status.state,
            running: instance.status.running,
            local_endpoints,
            published_endpoints: instance
                .exposed_endpoints
                .into_iter()
                .map(|endpoint| PublishedEndpointView {
                    name: endpoint.name,
                    local_address: format!("127.0.0.1:{}", endpoint.host_port),
                })
                .collect(),
        }
    }
}

impl From<ServiceInstance> for LocalServiceInspectVerboseView {
    fn from(instance: ServiceInstance) -> Self {
        let local_endpoints = local_endpoint_verbose_views(&instance);
        Self {
            id: instance.id,
            name: instance.name,
            runtime: instance.runtime,
            source: instance.source,
            labels: instance.labels,
            state: instance.status.state,
            running: instance.status.running,
            local_endpoints,
            published_endpoints: instance
                .exposed_endpoints
                .into_iter()
                .map(|endpoint| PublishedEndpointVerboseView {
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
            local_address: format!("127.0.0.1:{}", port.host_port),
        })
        .collect()
}

fn local_endpoint_verbose_views(
    instance: &ServiceInstance,
) -> Vec<LocalServiceEndpointVerboseView> {
    instance
        .ports
        .iter()
        .map(|port| LocalServiceEndpointVerboseView {
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
