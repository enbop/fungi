#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
use std::process::Command;

use clap::{Args, Subcommand};
use fungi_daemon::{
    CatalogService, RuntimeKind, ServiceAccess, ServiceExposeUsageKind, ServiceInstance,
    ServicePortProtocol,
};
use fungi_daemon_grpc::{
    Request,
    fungi_daemon_grpc::{
        AttachServiceAccessRequest, Empty, GetServiceLogsRequest, ListPeerCatalogRequest,
        ListServiceAccessesRequest, ListServicesResponse, PeerInfo, PullServiceRequest,
        RemotePeerRequest, RemotePullServiceRequest, RemoteServiceControlResponse,
        RemoteServiceNameRequest, ServiceInstanceResponse, ServiceNameRequest,
    },
};
use serde::Serialize;

use crate::commands::CommonArgs;

use super::{
    client::get_rpc_client,
    shared::{
        DeviceInput, OptionalDeviceTargetArg, fatal, fatal_grpc, print_target_device,
        resolve_optional_device,
    },
};

type RpcClient = fungi_daemon_grpc::fungi_daemon_grpc::fungi_daemon_client::FungiDaemonClient<
    tonic::transport::Channel,
>;
type RemoteService = CatalogService;

#[derive(Args, Debug, Clone)]
pub struct ServiceArgs {
    #[command(flatten)]
    pub device: OptionalDeviceTargetArg,
    /// Refresh remote service list from saved devices
    #[arg(long, default_value_t = false)]
    pub refresh: bool,
    #[command(subcommand)]
    pub command: Option<ServiceCommands>,
}

#[derive(Subcommand, Debug, Clone)]
pub enum ServiceCommands {
    /// List services on this node or another device
    List {
        /// Show detailed output
        #[arg(short, long, default_value_t = false)]
        verbose: bool,
        /// Refresh remote service list from saved devices
        #[arg(long, default_value_t = false)]
        refresh: bool,
    },
    /// Add a service manifest to this node or another device
    Add {
        /// Path to a service manifest YAML file
        manifest: String,
    },
    /// Open a service in the default local app when possible
    Open {
        service: String,
        entry: Option<String>,
    },
    /// Print or create a local connection address for a service
    Connect {
        service: String,
        entry: Option<String>,
    },
    /// Start a service by name on this node or another device
    Start { name: String },
    /// Stop a service by name on this node or another device
    Stop { name: String },
    /// Inspect a service by name on this node or another device
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
    /// Remove a service by name from this node or another device
    Remove { name: String },
    /// Deprecated: pull a service manifest onto the local node; use `service add`
    #[command(hide = true)]
    Pull {
        /// Path to a service manifest YAML file
        manifest: String,
    },
}

pub async fn execute_service(args: CommonArgs, service_args: ServiceArgs) {
    let mut client = match get_rpc_client(&args).await {
        Some(c) => c,
        None => fatal("Cannot connect to Fungi daemon. Is it running?"),
    };

    let device = match resolve_optional_device(&args, service_args.device.device.as_ref()) {
        Ok(device) => device,
        Err(error) => fatal(error),
    };

    let command = service_args.command.unwrap_or(ServiceCommands::List {
        verbose: false,
        refresh: false,
    });

    match command {
        ServiceCommands::List { verbose, refresh } => {
            if let Some(device) = device {
                print_target_device(&device);
                let req = RemotePeerRequest {
                    peer_id: device.peer_id,
                };
                match client.remote_list_services(Request::new(req)).await {
                    Ok(resp) => print_service_instances(resp.into_inner(), verbose),
                    Err(error) => fatal_grpc(error),
                }
            } else {
                print_service_overview(&mut client, verbose, service_args.refresh || refresh).await;
            }
        }
        ServiceCommands::Add { manifest } => {
            let (manifest_yaml, manifest_base_dir) = read_manifest_yaml_file(&manifest);
            if let Some(device) = device {
                print_target_device(&device);
                let req = RemotePullServiceRequest {
                    peer_id: device.peer_id,
                    manifest_yaml,
                };
                match client.remote_pull_service(Request::new(req)).await {
                    Ok(resp) => print_remote_service_added(resp.into_inner()),
                    Err(error) => fatal_grpc(error),
                }
            } else {
                let req = PullServiceRequest {
                    manifest_yaml,
                    manifest_base_dir,
                };
                match client.pull_service(Request::new(req)).await {
                    Ok(resp) => print_service_instance(resp.into_inner(), false),
                    Err(e) => fatal_grpc(e),
                }
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
            if let Some(device) = device {
                print_target_device(&device);
                let req = RemoteServiceNameRequest {
                    peer_id: device.peer_id,
                    name,
                };
                match client.remote_start_service(Request::new(req)).await {
                    Ok(resp) => print_remote_service_result("started", resp.into_inner()),
                    Err(error) => fatal_grpc(error),
                }
            } else {
                let req = ServiceNameRequest { runtime: 0, name };
                match client.start_service(Request::new(req)).await {
                    Ok(_) => println!("Service started"),
                    Err(e) => fatal_grpc(e),
                }
            }
        }
        ServiceCommands::Inspect { name, verbose } => {
            if let Some(device) = device {
                print_target_device(&device);
                let instance = inspect_remote_service(&mut client, &device.peer_id, name).await;
                print_service_instance_value(instance, verbose);
            } else {
                let req = ServiceNameRequest { runtime: 0, name };
                match client.inspect_service(Request::new(req)).await {
                    Ok(resp) => print_service_instance(resp.into_inner(), verbose),
                    Err(e) => fatal_grpc(e),
                }
            }
        }
        ServiceCommands::Logs { name, tail } => {
            if device.is_some() {
                fatal("Remote service logs are not implemented yet")
            }
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
            if let Some(device) = device {
                print_target_device(&device);
                let req = RemoteServiceNameRequest {
                    peer_id: device.peer_id,
                    name,
                };
                match client.remote_stop_service(Request::new(req)).await {
                    Ok(resp) => print_remote_service_result("stopped", resp.into_inner()),
                    Err(error) => fatal_grpc(error),
                }
            } else {
                let req = ServiceNameRequest { runtime: 0, name };
                match client.stop_service(Request::new(req)).await {
                    Ok(_) => println!("Service stopped"),
                    Err(e) => fatal_grpc(e),
                }
            }
        }
        ServiceCommands::Remove { name } => {
            if let Some(device) = device {
                print_target_device(&device);
                let req = RemoteServiceNameRequest {
                    peer_id: device.peer_id,
                    name,
                };
                match client.remote_remove_service(Request::new(req)).await {
                    Ok(resp) => print_remote_service_result("removed", resp.into_inner()),
                    Err(error) => fatal_grpc(error),
                }
            } else {
                let req = ServiceNameRequest { runtime: 0, name };
                match client.remove_service(Request::new(req)).await {
                    Ok(_) => println!("Service removed"),
                    Err(e) => fatal_grpc(e),
                }
            }
        }
        ServiceCommands::Open { service, entry } => {
            let url = if let Some(device) = device {
                print_target_device(&device);
                let remote_service =
                    discover_remote_service(&mut client, &device.peer_id, &service).await;
                let access =
                    existing_or_attach_access(&mut client, &device.peer_id, &service).await;
                build_web_url(&remote_service, &access, entry.as_deref())
            } else {
                let instance = inspect_local_service(&mut client, service).await;
                build_local_web_url(&instance, entry.as_deref())
            };

            let Some(url) = url else {
                fatal("No web entry is available for this service")
            };
            open_url(&url);
            println!("Opened {url}");
        }
        ServiceCommands::Connect { service, entry } => {
            let address = if let Some(device) = device {
                print_target_device(&device);
                let access =
                    existing_or_attach_access(&mut client, &device.peer_id, &service).await;
                select_access_endpoint(&access, entry.as_deref())
                    .map(|endpoint| format!("{}:{}", endpoint.local_host, endpoint.local_port))
            } else {
                let instance = inspect_local_service(&mut client, service).await;
                select_local_port(&instance, entry.as_deref())
                    .map(|port| format!("127.0.0.1:{}", port.host_port))
            };

            let Some(address) = address else {
                fatal("No connectable entry is available for this service")
            };
            println!("{address}");
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DynamicThingInvocation {
    pub target: DynamicThingTarget,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DynamicThingTarget {
    pub name: String,
    pub device: Option<DeviceInput>,
    pub entry: Option<String>,
}

pub async fn execute_dynamic_thing(
    args: CommonArgs,
    device_context: Option<DeviceInput>,
    tokens: Vec<String>,
) {
    let invocation = parse_dynamic_thing_invocation(tokens).unwrap_or_else(|error| fatal(error));

    if invocation.target.name.starts_with(':') {
        fatal("Shortcuts are not implemented yet")
    }

    if !invocation.args.is_empty() {
        fatal("Dynamic tool execution is not implemented yet")
    }

    if device_context.is_some() && invocation.target.device.is_some() {
        fatal("Device specified twice. Use either -d <device> or thing@device.")
    }

    let device = invocation.target.device.or(device_context);
    if device.is_none() {
        open_dynamic_service_without_device(args, invocation.target.name, invocation.target.entry)
            .await;
        return;
    }

    execute_service(
        args,
        ServiceArgs {
            device: OptionalDeviceTargetArg { device },
            refresh: false,
            command: Some(ServiceCommands::Open {
                service: invocation.target.name,
                entry: invocation.target.entry,
            }),
        },
    )
    .await;
}

pub fn parse_dynamic_thing_invocation(
    mut tokens: Vec<String>,
) -> Result<DynamicThingInvocation, String> {
    if tokens.is_empty() {
        return Err("Missing thing name".to_string());
    }

    let target = parse_dynamic_thing_target(tokens.remove(0))?;
    Ok(DynamicThingInvocation {
        target,
        args: tokens,
    })
}

pub fn parse_dynamic_thing_target(value: String) -> Result<DynamicThingTarget, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("Thing name cannot be empty".to_string());
    }

    let (name_and_device, entry) = match value.split_once('/') {
        Some((head, tail)) => {
            if tail.trim().is_empty() {
                return Err("Entry name cannot be empty".to_string());
            }
            (head, Some(tail.to_string()))
        }
        None => (value, None),
    };

    let (name, device) =
        match name_and_device.split_once('@') {
            Some((name, device)) => {
                if name.trim().is_empty() {
                    return Err("Thing name cannot be empty".to_string());
                }
                if device.trim().is_empty() {
                    return Err("Device name cannot be empty".to_string());
                }
                if device.contains('@') {
                    return Err("Thing target can only include one @device suffix".to_string());
                }
                (
                    name.to_string(),
                    Some(device.parse::<DeviceInput>().map_err(|error| {
                        format!("Invalid device in thing target {value}: {error}")
                    })?),
                )
            }
            None => (name_and_device.to_string(), None),
        };

    Ok(DynamicThingTarget {
        name,
        device,
        entry,
    })
}

fn print_remote_service_added(
    resp: fungi_daemon_grpc::fungi_daemon_grpc::RemoteServiceControlResponse,
) {
    let service_name = if resp.service_name.trim().is_empty() {
        "<unknown>"
    } else {
        resp.service_name.as_str()
    };
    println!("Remote service added: {service_name}");
}

fn print_remote_service_result(action: &str, resp: RemoteServiceControlResponse) {
    let service_name = if resp.service_name.trim().is_empty() {
        "<unknown>"
    } else {
        resp.service_name.as_str()
    };
    println!("Remote service {action}: {service_name}");
}

async fn inspect_local_service(client: &mut RpcClient, name: String) -> ServiceInstance {
    let req = ServiceNameRequest { runtime: 0, name };
    match client.inspect_service(Request::new(req)).await {
        Ok(resp) => match serde_json::from_str::<ServiceInstance>(&resp.into_inner().instance_json)
        {
            Ok(instance) => instance,
            Err(error) => fatal(format!("Failed to decode service instance: {error}")),
        },
        Err(error) => fatal_grpc(error),
    }
}

async fn open_dynamic_service_without_device(
    args: CommonArgs,
    service: String,
    entry: Option<String>,
) {
    let mut client = match get_rpc_client(&args).await {
        Some(c) => c,
        None => fatal("Cannot connect to Fungi daemon. Is it running?"),
    };

    if let Some(instance) = find_local_service(&mut client, &service).await {
        let Some(url) = build_local_web_url(&instance, entry.as_deref()) else {
            fatal("No web entry is available for this service")
        };
        open_url(&url);
        println!("Opened {url}");
        return;
    }

    if open_unique_cached_remote_service(&mut client, &service, entry.as_deref()).await {
        return;
    }

    fatal(format!(
        "Service not found in local services or cached remote access: {service}\nRun `fungi service --refresh` to refresh remote services, then use `fungi <service>@<device>` if needed."
    ));
}

async fn find_local_service(client: &mut RpcClient, service: &str) -> Option<ServiceInstance> {
    list_local_service_instances(client)
        .await
        .into_iter()
        .find(|instance| instance.name == service || instance.id == service)
}

async fn print_service_overview(client: &mut RpcClient, verbose: bool, refresh: bool) {
    let mut rows = Vec::new();

    let local_services = list_local_service_instances(client).await;
    rows.extend(
        local_services
            .into_iter()
            .map(|service| ServiceOverviewRow::from_local(service, verbose)),
    );

    let devices = list_saved_devices(client).await;
    if refresh {
        for device in devices {
            let services = match fetch_remote_services(client, &device.peer_id).await {
                Ok(services) => services,
                Err(error) => {
                    rows.push(ServiceOverviewRow::remote_unavailable(&device, error));
                    continue;
                }
            };

            let attached = list_accesses(client, &device.peer_id).await;
            rows.extend(services.into_iter().map(|service| {
                ServiceOverviewRow::from_remote_service(service, &device, &attached, verbose)
            }));
        }
    } else {
        for device in devices {
            let accesses = list_accesses(client, &device.peer_id).await;
            rows.extend(
                accesses
                    .into_iter()
                    .map(|access| ServiceOverviewRow::from_cached_access(access, &device, verbose)),
            );
        }
    }

    rows.sort_by(|left, right| left.reference.cmp(&right.reference));
    print_service_overview_rows(&rows);
}

async fn list_local_service_instances(client: &mut RpcClient) -> Vec<ServiceInstance> {
    match client.list_services(Request::new(Empty {})).await {
        Ok(resp) => decode_service_instances(resp.into_inner()),
        Err(error) => fatal_grpc(error),
    }
}

async fn list_saved_devices(client: &mut RpcClient) -> Vec<PeerInfo> {
    match client.list_address_book_peers(Request::new(Empty {})).await {
        Ok(resp) => resp.into_inner().peers,
        Err(error) => fatal_grpc(error),
    }
}

async fn list_remote_service_instances(
    client: &mut RpcClient,
    peer_id: &str,
) -> Vec<ServiceInstance> {
    let req = RemotePeerRequest {
        peer_id: peer_id.to_string(),
    };
    match client.remote_list_services(Request::new(req)).await {
        Ok(resp) => {
            match serde_json::from_str::<Vec<ServiceInstance>>(&resp.into_inner().services_json) {
                Ok(services) => services,
                Err(error) => fatal(format!("Failed to decode remote service list: {error}")),
            }
        }
        Err(error) => fatal_grpc(error),
    }
}

async fn fetch_remote_services(
    client: &mut RpcClient,
    peer_id: &str,
) -> Result<Vec<RemoteService>, String> {
    let req = ListPeerCatalogRequest {
        peer_id: peer_id.to_string(),
    };
    match client.list_peer_catalog(Request::new(req)).await {
        Ok(resp) => serde_json::from_str::<Vec<RemoteService>>(&resp.into_inner().services_json)
            .map_err(|error| format!("Failed to decode remote services: {error}")),
        Err(error) => Err(error.message().to_string()),
    }
}

#[derive(Debug, Clone)]
struct CachedRemoteServiceMatch {
    device: PeerInfo,
    access: ServiceAccess,
}

async fn open_unique_cached_remote_service(
    client: &mut RpcClient,
    service: &str,
    entry: Option<&str>,
) -> bool {
    let devices = list_saved_devices(client).await;
    let mut matches = Vec::new();

    for device in devices {
        for access in list_accesses(client, &device.peer_id).await {
            if service_matches(&access.service_id, &access.service_name, service) {
                matches.push(CachedRemoteServiceMatch {
                    device: device.clone(),
                    access,
                });
            }
        }
    }

    match matches.len() {
        0 => false,
        1 => {
            let matched = matches.remove(0);
            let device_name = device_display_name(&matched.device);
            let reference = format!("{}@{}", matched.access.service_id, device_name);
            let Some(url) = build_cached_access_web_url(&matched.access, entry) else {
                fatal(format!(
                    "Matched {reference}, but it has no cached web entry"
                ))
            };
            println!("Matched {reference}");
            open_url(&url);
            println!("Opened {url}");
            true
        }
        _ => {
            fatal(format!(
                "Multiple remote services named {service}. Use one of:\n{}",
                format_cached_remote_candidates(&matches)
            ));
        }
    }
}

fn service_matches(service_id: &str, service_name: &str, value: &str) -> bool {
    service_id == value || service_name == value
}

fn build_cached_access_web_url(access: &ServiceAccess, entry: Option<&str>) -> Option<String> {
    let endpoint = if let Some(entry) = entry {
        access
            .endpoints
            .iter()
            .find(|endpoint| endpoint.name == entry)?
    } else {
        access
            .endpoints
            .iter()
            .find(|endpoint| is_web_entry_name(Some(endpoint.name.as_str())))?
    };

    Some(format!(
        "http://{}:{}",
        endpoint.local_host, endpoint.local_port
    ))
}

fn format_cached_remote_candidates(matches: &[CachedRemoteServiceMatch]) -> String {
    matches
        .iter()
        .map(|matched| {
            format!(
                "  {}@{}",
                matched.access.service_id,
                device_display_name(&matched.device)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

async fn inspect_remote_service(
    client: &mut RpcClient,
    peer_id: &str,
    name: String,
) -> ServiceInstance {
    list_remote_service_instances(client, peer_id)
        .await
        .into_iter()
        .find(|instance| instance.name == name)
        .unwrap_or_else(|| fatal(format!("Remote service not found: {name}")))
}

async fn list_accesses(client: &mut RpcClient, peer_id: &str) -> Vec<ServiceAccess> {
    let req = ListServiceAccessesRequest {
        peer_id: peer_id.to_string(),
    };
    match client.list_service_accesses(Request::new(req)).await {
        Ok(resp) => match serde_json::from_str::<Vec<ServiceAccess>>(
            &resp.into_inner().service_accesses_json,
        ) {
            Ok(accesses) => accesses,
            Err(error) => fatal(format!("Failed to decode access list: {error}")),
        },
        Err(error) => fatal_grpc(error),
    }
}

async fn attach_access(client: &mut RpcClient, peer_id: &str, service_id: &str) -> ServiceAccess {
    let req = AttachServiceAccessRequest {
        peer_id: peer_id.to_string(),
        service_id: service_id.to_string(),
    };
    match client.attach_service_access(Request::new(req)).await {
        Ok(resp) => {
            match serde_json::from_str::<ServiceAccess>(&resp.into_inner().service_access_json) {
                Ok(access) => access,
                Err(error) => fatal(format!("Failed to decode access entry: {error}")),
            }
        }
        Err(error) => fatal_grpc(error),
    }
}

async fn existing_or_attach_access(
    client: &mut RpcClient,
    peer_id: &str,
    service_id: &str,
) -> ServiceAccess {
    let existing = list_accesses(client, peer_id).await;
    if let Some(access) = existing
        .into_iter()
        .find(|access| access.service_id == service_id)
    {
        return access;
    }

    attach_access(client, peer_id, service_id).await
}

async fn discover_remote_service(
    client: &mut RpcClient,
    peer_id: &str,
    service_id: &str,
) -> RemoteService {
    let services = match fetch_remote_services(client, peer_id).await {
        Ok(services) => services,
        Err(error) => fatal(error),
    };

    services
        .into_iter()
        .find(|service| service.service_id == service_id)
        .unwrap_or_else(|| fatal(format!("Remote service not found: {service_id}")))
}

fn build_web_url(
    service: &RemoteService,
    access: &ServiceAccess,
    entry: Option<&str>,
) -> Option<String> {
    if !matches!(
        service.usage.as_ref().map(|usage| usage.kind),
        Some(ServiceExposeUsageKind::Web)
    ) {
        return None;
    }

    let endpoint = select_access_endpoint(access, entry)?;
    let mut value = format!("http://{}:{}", endpoint.local_host, endpoint.local_port);
    if let Some(path) = service
        .usage
        .as_ref()
        .and_then(|usage| usage.path.as_deref())
        && !path.is_empty()
    {
        if path.starts_with('/') {
            value.push_str(path);
        } else {
            value.push('/');
            value.push_str(path);
        }
    }
    Some(value)
}

fn build_local_web_url(instance: &ServiceInstance, entry: Option<&str>) -> Option<String> {
    select_local_web_port(instance, entry)
        .map(|port| format!("http://127.0.0.1:{}", port.host_port))
}

fn select_access_endpoint<'a>(
    access: &'a ServiceAccess,
    entry: Option<&str>,
) -> Option<&'a fungi_daemon::ServiceAccessEndpoint> {
    if let Some(entry) = entry {
        return access
            .endpoints
            .iter()
            .find(|endpoint| endpoint.name == entry);
    }

    access
        .endpoints
        .iter()
        .find(|endpoint| endpoint.name == "web")
        .or_else(|| {
            access
                .endpoints
                .iter()
                .find(|endpoint| endpoint.name == "main")
        })
        .or_else(|| access.endpoints.first())
}

fn select_local_port<'a>(
    instance: &'a ServiceInstance,
    entry: Option<&str>,
) -> Option<&'a fungi_daemon::ServicePort> {
    if let Some(entry) = entry {
        return instance
            .ports
            .iter()
            .find(|port| port.name.as_deref() == Some(entry));
    }

    instance
        .ports
        .iter()
        .find(|port| port.name.as_deref() == Some("web"))
        .or_else(|| {
            instance
                .ports
                .iter()
                .find(|port| port.name.as_deref() == Some("main"))
        })
        .or_else(|| instance.ports.first())
}

fn select_local_web_port<'a>(
    instance: &'a ServiceInstance,
    entry: Option<&str>,
) -> Option<&'a fungi_daemon::ServicePort> {
    if let Some(entry) = entry {
        return select_local_port(instance, Some(entry));
    }

    instance
        .ports
        .iter()
        .find(|port| is_web_entry_name(port.name.as_deref()))
}

fn is_web_entry_name(name: Option<&str>) -> bool {
    matches!(
        name.map(|value| value.trim().to_ascii_lowercase()),
        Some(value) if matches!(value.as_str(), "web" | "http" | "https")
    )
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn run_url_opener(mut command: Command) {
    match command.status() {
        Ok(result) if result.success() => {}
        Ok(result) => fatal(format!("Failed to open URL, exit code: {result}")),
        Err(error) => fatal(format!("Failed to launch URL opener: {error}")),
    }
}

#[cfg(target_os = "macos")]
fn open_url(url: &str) {
    let mut command = Command::new("open");
    command.arg(url);
    run_url_opener(command);
}

#[cfg(target_os = "linux")]
fn open_url(url: &str) {
    let mut command = Command::new("xdg-open");
    command.arg(url);
    run_url_opener(command);
}

#[cfg(target_os = "windows")]
fn open_url(url: &str) {
    let mut command = Command::new("cmd");
    command.args(["/C", "start", "", url]);
    run_url_opener(command);
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn open_url(_url: &str) {
    fatal("Opening URLs is not supported on this platform")
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
        Ok(instance) => print_service_instance_value(instance, verbose),
        Err(error) => fatal(format!("Failed to decode service instance: {error}")),
    }
}

pub(crate) fn print_service_instances(resp: ListServicesResponse, verbose: bool) {
    print_service_instances_value(decode_service_instances(resp), verbose)
}

fn decode_service_instances(resp: ListServicesResponse) -> Vec<ServiceInstance> {
    match serde_json::from_str::<Vec<ServiceInstance>>(&resp.services_json) {
        Ok(services) => services,
        Err(error) => fatal(format!("Failed to decode service list: {error}")),
    }
}

fn print_service_instance_value(instance: ServiceInstance, verbose: bool) {
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

fn print_service_instances_value(services: Vec<ServiceInstance>, verbose: bool) {
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

#[derive(Debug, Clone)]
struct ServiceOverviewRow {
    reference: String,
    device: String,
    kind: String,
    state: String,
    entries: Vec<String>,
    note: Option<String>,
}

impl ServiceOverviewRow {
    fn from_local(service: ServiceInstance, verbose: bool) -> Self {
        let entries = if verbose {
            service
                .ports
                .iter()
                .map(|port| {
                    let name = port.name.clone().unwrap_or_else(|| "main".to_string());
                    format!("{name}:127.0.0.1:{}", port.host_port)
                })
                .collect()
        } else {
            service
                .ports
                .iter()
                .map(|port| port.name.clone().unwrap_or_else(|| "main".to_string()))
                .collect()
        };

        Self {
            reference: service.name.clone(),
            device: "this".to_string(),
            kind: "local".to_string(),
            state: service.status.state,
            entries,
            note: None,
        }
    }

    fn from_cached_access(access: ServiceAccess, device: &PeerInfo, verbose: bool) -> Self {
        let device_name = device_display_name(device);
        let entries = if verbose {
            access
                .endpoints
                .iter()
                .map(|endpoint| {
                    format!(
                        "{}:{}:{}",
                        endpoint.name, endpoint.local_host, endpoint.local_port
                    )
                })
                .collect()
        } else {
            access
                .endpoints
                .iter()
                .map(|endpoint| endpoint.name.clone())
                .collect()
        };

        Self {
            reference: format!("{}@{}", access.service_id, device_name),
            device: device_name,
            kind: "cached-access".to_string(),
            state: "connected".to_string(),
            entries,
            note: None,
        }
    }

    fn from_remote_service(
        service: RemoteService,
        device: &PeerInfo,
        attached: &[ServiceAccess],
        verbose: bool,
    ) -> Self {
        let device_name = device_display_name(device);
        let attached_access = attached
            .iter()
            .find(|access| access.service_id == service.service_id);
        let entries = if verbose {
            match attached_access {
                Some(access) => access
                    .endpoints
                    .iter()
                    .map(|endpoint| {
                        format!(
                            "{}:{}:{}",
                            endpoint.name, endpoint.local_host, endpoint.local_port
                        )
                    })
                    .collect(),
                None => service
                    .endpoints
                    .iter()
                    .map(|endpoint| format!("{}:{}", endpoint.name, endpoint.service_port))
                    .collect(),
            }
        } else {
            service
                .endpoints
                .iter()
                .map(|endpoint| endpoint.name.clone())
                .collect()
        };

        Self {
            reference: format!("{}@{}", service.service_id, device_name),
            device: device_name,
            kind: "remote".to_string(),
            state: service.status.state,
            entries,
            note: attached_access.map(|_| "attached".to_string()),
        }
    }

    fn remote_unavailable(device: &PeerInfo, error: String) -> Self {
        let device_name = device_display_name(device);
        Self {
            reference: format!("@{device_name}"),
            device: device_name,
            kind: "remote".to_string(),
            state: "unavailable".to_string(),
            entries: Vec::new(),
            note: Some(error),
        }
    }
}

fn print_service_overview_rows(rows: &[ServiceOverviewRow]) {
    if rows.is_empty() {
        println!("No services found");
        return;
    }

    let ref_width = rows
        .iter()
        .map(|row| row.reference.len())
        .max()
        .unwrap_or("SERVICE".len())
        .max("SERVICE".len());
    let device_width = rows
        .iter()
        .map(|row| row.device.len())
        .max()
        .unwrap_or("DEVICE".len())
        .max("DEVICE".len());
    let kind_width = rows
        .iter()
        .map(|row| row.kind.len())
        .max()
        .unwrap_or("KIND".len())
        .max("KIND".len());
    let state_width = rows
        .iter()
        .map(|row| row.state.len())
        .max()
        .unwrap_or("STATE".len())
        .max("STATE".len());

    println!(
        "{:<ref_width$}  {:<device_width$}  {:<kind_width$}  {:<state_width$}  ENTRIES",
        "SERVICE", "DEVICE", "KIND", "STATE"
    );
    for row in rows {
        let entries = if row.entries.is_empty() {
            "-".to_string()
        } else {
            row.entries.join(",")
        };
        let suffix = row
            .note
            .as_ref()
            .map(|note| format!("  {note}"))
            .unwrap_or_default();
        println!(
            "{:<ref_width$}  {:<device_width$}  {:<kind_width$}  {:<state_width$}  {}{}",
            row.reference, row.device, row.kind, row.state, entries, suffix
        );
    }
}

fn device_display_name(device: &PeerInfo) -> String {
    if !device.alias.trim().is_empty() {
        device.alias.clone()
    } else if !device.hostname.trim().is_empty() {
        device.hostname.clone()
    } else {
        device.peer_id.clone()
    }
}

#[derive(Debug, Serialize)]
struct LocalServiceListEntry {
    service_name: String,
    state: String,
    running: bool,
    entries: Vec<ServiceEntryView>,
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
    entries: Vec<ServiceEntryView>,
    published_entries: Vec<ServiceEntryView>,
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
struct ServiceEntryView {
    name: Option<String>,
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
struct PublishedEndpointVerboseView {
    name: String,
    protocol: String,
    local_host: String,
    local_port: u16,
    service_port: u16,
}

impl From<ServiceInstance> for LocalServiceListEntry {
    fn from(instance: ServiceInstance) -> Self {
        let entries = local_entry_views(&instance);
        Self {
            service_name: instance.name,
            state: instance.status.state,
            running: instance.status.running,
            entries,
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
        let entries = local_entry_views(&instance);
        Self {
            name: instance.name,
            state: instance.status.state,
            running: instance.status.running,
            entries,
            published_entries: instance
                .exposed_endpoints
                .into_iter()
                .map(|endpoint| ServiceEntryView {
                    name: Some(endpoint.name),
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

fn local_entry_views(instance: &ServiceInstance) -> Vec<ServiceEntryView> {
    instance
        .ports
        .iter()
        .map(|port| ServiceEntryView {
            name: port.name.clone(),
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use fungi_daemon::{
        ServiceAccessEndpoint, ServiceExposeTransport, ServiceExposeTransportKind,
        ServiceExposeUsage, ServicePort, ServicePortAllocation, ServiceStatus,
    };

    use super::*;

    #[test]
    fn select_access_endpoint_prefers_requested_entry() {
        let access = service_access(vec![
            access_endpoint("web", 28080),
            access_endpoint("admin", 28081),
        ]);

        let endpoint = select_access_endpoint(&access, Some("admin")).unwrap();

        assert_eq!(endpoint.name, "admin");
        assert_eq!(endpoint.local_port, 28081);
    }

    #[test]
    fn select_access_endpoint_defaults_to_web_then_main() {
        let access = service_access(vec![
            access_endpoint("main", 28080),
            access_endpoint("web", 28081),
        ]);

        let endpoint = select_access_endpoint(&access, None).unwrap();

        assert_eq!(endpoint.name, "web");
        assert_eq!(endpoint.local_port, 28081);
    }

    #[test]
    fn build_web_url_uses_selected_entry_and_usage_path() {
        let access = service_access(vec![
            access_endpoint("web", 28080),
            access_endpoint("admin", 28081),
        ]);
        let service = remote_web_service("/dashboard");

        let url = build_web_url(&service, &access, Some("admin")).unwrap();

        assert_eq!(url, "http://127.0.0.1:28081/dashboard");
    }

    #[test]
    fn select_local_port_defaults_to_web() {
        let instance = service_instance(vec![
            service_port("main", 28080),
            service_port("web", 28081),
        ]);

        let port = select_local_port(&instance, None).unwrap();

        assert_eq!(port.name.as_deref(), Some("web"));
        assert_eq!(port.host_port, 28081);
    }

    #[test]
    fn build_local_web_url_defaults_to_http_named_port() {
        let instance = service_instance(vec![
            service_port("api", 28080),
            service_port("http", 28081),
        ]);

        let url = build_local_web_url(&instance, None).unwrap();

        assert_eq!(url, "http://127.0.0.1:28081");
    }

    #[test]
    fn build_local_web_url_rejects_non_web_default() {
        let instance =
            service_instance(vec![service_port("ssh", 28022), service_port("api", 28080)]);

        let url = build_local_web_url(&instance, None);

        assert!(url.is_none());
    }

    #[test]
    fn build_local_web_url_allows_explicit_entry() {
        let instance = service_instance(vec![service_port("admin", 28081)]);

        let url = build_local_web_url(&instance, Some("admin")).unwrap();

        assert_eq!(url, "http://127.0.0.1:28081");
    }

    #[test]
    fn build_cached_access_web_url_defaults_to_web_endpoint() {
        let access = service_access(vec![
            access_endpoint("ssh", 28022),
            access_endpoint("web", 28080),
        ]);

        let url = build_cached_access_web_url(&access, None).unwrap();

        assert_eq!(url, "http://127.0.0.1:28080");
    }

    #[test]
    fn build_cached_access_web_url_rejects_non_web_default() {
        let access = service_access(vec![
            access_endpoint("ssh", 28022),
            access_endpoint("api", 28080),
        ]);

        let url = build_cached_access_web_url(&access, None);

        assert!(url.is_none());
    }

    #[test]
    fn service_matches_id_or_name() {
        assert!(service_matches(
            "filebrowser",
            "File Browser",
            "filebrowser"
        ));
        assert!(service_matches("fb", "filebrowser", "filebrowser"));
        assert!(!service_matches("fb", "File Browser", "filebrowser"));
    }

    #[test]
    fn default_service_list_view_hides_local_ports() {
        let instance =
            service_instance(vec![service_port("web", 28080), service_port("api", 28081)]);

        let view = LocalServiceListEntry::from(instance);
        let json = serde_json::to_value(view).unwrap();
        let text = serde_json::to_string(&json).unwrap();

        assert_eq!(
            json["entries"],
            serde_json::json!([
                { "name": "web" },
                { "name": "api" }
            ])
        );
        assert!(json.get("local_endpoints").is_none());
        assert!(!text.contains("127.0.0.1"));
        assert!(!text.contains("28080"));
        assert!(!text.contains("28081"));
    }

    #[test]
    fn default_service_inspect_view_hides_local_ports() {
        let mut instance = service_instance(vec![service_port("web", 28080)]);
        instance.exposed_endpoints = vec![fungi_daemon::ServiceExposeEndpointBinding {
            name: "web".to_string(),
            protocol: "/fungi/service/demo/web/0.2.0".to_string(),
            host_port: 28080,
            service_port: 80,
        }];

        let view = LocalServiceInspectView::from(instance);
        let json = serde_json::to_value(view).unwrap();
        let text = serde_json::to_string(&json).unwrap();

        assert_eq!(json["entries"], serde_json::json!([{ "name": "web" }]));
        assert_eq!(
            json["published_entries"],
            serde_json::json!([{ "name": "web" }])
        );
        assert!(json.get("local_endpoints").is_none());
        assert!(json.get("published_endpoints").is_none());
        assert!(!text.contains("127.0.0.1"));
        assert!(!text.contains("28080"));
    }

    #[test]
    fn parse_dynamic_thing_target_supports_device_and_entry() {
        let target = parse_dynamic_thing_target("filebrowser@nas/admin".to_string()).unwrap();

        assert_eq!(target.name, "filebrowser");
        assert!(matches!(target.device, Some(DeviceInput::Alias(alias)) if alias == "nas"));
        assert_eq!(target.entry.as_deref(), Some("admin"));
    }

    #[test]
    fn parse_dynamic_thing_invocation_keeps_tool_args() {
        let invocation = parse_dynamic_thing_invocation(vec![
            "rg@nas".to_string(),
            "todo".to_string(),
            "/data".to_string(),
        ])
        .unwrap();

        assert_eq!(invocation.target.name, "rg");
        assert!(
            matches!(invocation.target.device, Some(DeviceInput::Alias(alias)) if alias == "nas")
        );
        assert_eq!(invocation.args, vec!["todo", "/data"]);
    }

    #[test]
    fn parse_dynamic_thing_target_rejects_empty_device() {
        let result = parse_dynamic_thing_target("filebrowser@".to_string());

        assert!(result.is_err());
    }

    fn service_access(endpoints: Vec<ServiceAccessEndpoint>) -> ServiceAccess {
        ServiceAccess {
            peer_id: "peer".to_string(),
            service_id: "demo".to_string(),
            service_name: "demo".to_string(),
            endpoints,
        }
    }

    fn access_endpoint(name: &str, local_port: u16) -> ServiceAccessEndpoint {
        ServiceAccessEndpoint {
            name: name.to_string(),
            protocol: format!("/fungi/service/demo/{name}/0.2.0"),
            local_host: "127.0.0.1".to_string(),
            local_port,
        }
    }

    fn remote_web_service(path: &str) -> RemoteService {
        RemoteService {
            service_name: "demo".to_string(),
            service_id: "demo".to_string(),
            display_name: "Demo".to_string(),
            runtime: RuntimeKind::Docker,
            transport: ServiceExposeTransport {
                kind: ServiceExposeTransportKind::Tcp,
            },
            usage: Some(ServiceExposeUsage {
                kind: ServiceExposeUsageKind::Web,
                path: Some(path.to_string()),
            }),
            icon_url: None,
            catalog_id: None,
            endpoints: Vec::new(),
            status: ServiceStatus {
                state: "running".to_string(),
                running: true,
            },
        }
    }

    fn service_instance(ports: Vec<ServicePort>) -> ServiceInstance {
        ServiceInstance {
            id: "docker:demo".to_string(),
            runtime: RuntimeKind::Docker,
            name: "demo".to_string(),
            source: "demo:latest".to_string(),
            labels: BTreeMap::new(),
            ports,
            exposed_endpoints: Vec::new(),
            status: ServiceStatus {
                state: "running".to_string(),
                running: true,
            },
        }
    }

    fn service_port(name: &str, host_port: u16) -> ServicePort {
        ServicePort {
            name: Some(name.to_string()),
            host_port,
            host_port_allocation: ServicePortAllocation::Auto,
            service_port: 80,
            protocol: ServicePortProtocol::Tcp,
        }
    }
}
