#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
use std::process::Command;

use clap::Subcommand;
use fungi_daemon::{CatalogService, ServiceAccess, ServiceExposeUsageKind};
use fungi_daemon_grpc::{
    Request,
    fungi_daemon_grpc::{
        AttachServiceAccessRequest, DetachServiceAccessRequest, ListPeerCatalogRequest,
        ListServiceAccessesRequest,
    },
};
use serde::Serialize;

use crate::commands::CommonArgs;

use super::{
    client::get_rpc_client,
    shared::{
        OptionalPeerTargetArg, PeerTargetArg, fatal, fatal_grpc, print_target_peer,
        resolve_optional_peer, resolve_required_peer,
    },
};

#[derive(Subcommand, Debug, Clone)]
pub enum AccessCommands {
    /// List local access entries created for published remote services
    List {
        #[command(flatten)]
        peer: OptionalPeerTargetArg,
        /// Show detailed output
        #[arg(short, long, default_value_t = false)]
        verbose: bool,
    },
    /// Create a reusable local access entry for a published remote service
    Attach {
        /// Published service identifier
        service_id: String,
        #[command(flatten)]
        peer: PeerTargetArg,
        /// Show detailed output
        #[arg(short, long, default_value_t = false)]
        verbose: bool,
    },
    /// Remove a local access entry for a published remote service
    Detach {
        /// Published service identifier
        service_id: String,
        #[command(flatten)]
        peer: PeerTargetArg,
    },
    /// Open a published remote service, creating access if needed
    Open {
        /// Published service identifier
        service_id: String,
        #[command(flatten)]
        peer: PeerTargetArg,
        /// Create only a temporary access entry for this open operation
        #[arg(long, default_value_t = false)]
        ephemeral: bool,
    },
}

pub async fn execute_access(args: CommonArgs, cmd: AccessCommands) {
    let mut client = match get_rpc_client(&args).await {
        Some(c) => c,
        None => fatal("Cannot connect to Fungi daemon. Is it running?"),
    };

    match cmd {
        AccessCommands::List { peer, verbose } => {
            let peer = match resolve_optional_peer(&args, peer.peer.as_ref()) {
                Ok(peer) => peer,
                Err(error) => fatal(error),
            };
            if let Some(peer) = peer.as_ref() {
                print_target_peer(peer);
            }
            let accesses =
                list_accesses(&mut client, peer.as_ref().map(|peer| peer.peer_id.as_str())).await;
            print_accesses(&accesses, verbose);
        }
        AccessCommands::Attach {
            service_id,
            peer,
            verbose,
        } => {
            let peer = match resolve_required_peer(&args, peer.peer.as_ref()) {
                Ok(peer) => peer,
                Err(error) => fatal(error),
            };
            print_target_peer(&peer);
            let access = attach_access(&mut client, &peer.peer_id, &service_id).await;
            print_access(&access, verbose);
        }
        AccessCommands::Detach { service_id, peer } => {
            let peer = match resolve_required_peer(&args, peer.peer.as_ref()) {
                Ok(peer) => peer,
                Err(error) => fatal(error),
            };
            print_target_peer(&peer);
            let req = DetachServiceAccessRequest {
                peer_id: peer.peer_id,
                service_id,
            };
            match client.detach_service_access(Request::new(req)).await {
                Ok(_) => println!("Access entry detached"),
                Err(error) => fatal_grpc(error),
            }
        }
        AccessCommands::Open {
            service_id,
            peer,
            ephemeral,
        } => {
            if ephemeral {
                fatal("Ephemeral access is not implemented yet")
            }

            let peer = match resolve_required_peer(&args, peer.peer.as_ref()) {
                Ok(peer) => peer,
                Err(error) => fatal(error),
            };
            print_target_peer(&peer);

            let catalog = discover_catalog_service(&mut client, &peer.peer_id, &service_id).await;
            let access = existing_or_attach_access(&mut client, &peer.peer_id, &service_id).await;
            let urls = build_local_urls(&catalog, &access);
            let Some(url) = urls.into_iter().next() else {
                fatal("No local URL is available for this access entry")
            };
            open_url(&url);
            println!("Opened {url}");
        }
    }
}

async fn list_accesses(
    client: &mut fungi_daemon_grpc::fungi_daemon_grpc::fungi_daemon_client::FungiDaemonClient<
        tonic::transport::Channel,
    >,
    peer_id: Option<&str>,
) -> Vec<ServiceAccess> {
    let req = ListServiceAccessesRequest {
        peer_id: peer_id.unwrap_or_default().to_string(),
    };
    match client.list_service_accesses(Request::new(req)).await {
        Ok(resp) => match serde_json::from_str::<Vec<ServiceAccess>>(
            &resp.into_inner().service_accesses_json,
        ) {
            Ok(services) => services,
            Err(error) => fatal(format!("Failed to decode access list: {error}")),
        },
        Err(error) => fatal_grpc(error),
    }
}

async fn attach_access(
    client: &mut fungi_daemon_grpc::fungi_daemon_grpc::fungi_daemon_client::FungiDaemonClient<
        tonic::transport::Channel,
    >,
    peer_id: &str,
    service_id: &str,
) -> ServiceAccess {
    let req = AttachServiceAccessRequest {
        peer_id: peer_id.to_string(),
        service_id: service_id.to_string(),
        entry: String::new(),
        local_port: 0,
    };
    match client.attach_service_access(Request::new(req)).await {
        Ok(resp) => {
            match serde_json::from_str::<ServiceAccess>(&resp.into_inner().service_access_json) {
                Ok(service) => service,
                Err(error) => fatal(format!("Failed to decode access entry: {error}")),
            }
        }
        Err(error) => fatal_grpc(error),
    }
}

async fn existing_or_attach_access(
    client: &mut fungi_daemon_grpc::fungi_daemon_grpc::fungi_daemon_client::FungiDaemonClient<
        tonic::transport::Channel,
    >,
    peer_id: &str,
    service_id: &str,
) -> ServiceAccess {
    let existing = list_accesses(client, Some(peer_id)).await;
    if let Some(access) = existing
        .into_iter()
        .find(|access| access.service_id == service_id)
    {
        return access;
    }

    attach_access(client, peer_id, service_id).await
}

async fn discover_catalog_service(
    client: &mut fungi_daemon_grpc::fungi_daemon_grpc::fungi_daemon_client::FungiDaemonClient<
        tonic::transport::Channel,
    >,
    peer_id: &str,
    service_id: &str,
) -> CatalogService {
    let req = ListPeerCatalogRequest {
        peer_id: peer_id.to_string(),
        cached: false,
    };
    let services = match client.list_peer_catalog(Request::new(req)).await {
        Ok(resp) => {
            match serde_json::from_str::<Vec<CatalogService>>(&resp.into_inner().services_json) {
                Ok(services) => services,
                Err(error) => fatal(format!("Failed to decode catalog services: {error}")),
            }
        }
        Err(error) => fatal_grpc(error),
    };

    services
        .into_iter()
        .find(|service| service.service_id == service_id)
        .unwrap_or_else(|| fatal(format!("Published service not found: {service_id}")))
}

fn print_accesses(accesses: &[ServiceAccess], verbose: bool) {
    if verbose {
        let rows = accesses
            .iter()
            .map(|access| AccessListVerboseEntry {
                peer_id: access.peer_id.clone(),
                service_id: access.service_id.clone(),
                service_name: access.service_name.clone(),
                endpoints: access
                    .endpoints
                    .iter()
                    .map(|endpoint| AccessEndpointVerboseView {
                        name: endpoint.name.clone(),
                        protocol: endpoint.protocol.clone(),
                        local_host: endpoint.local_host.clone(),
                        local_port: endpoint.local_port,
                    })
                    .collect(),
            })
            .collect::<Vec<_>>();

        print_json(&rows, "access list")
    } else {
        let rows = accesses
            .iter()
            .map(|access| AccessListEntry {
                peer_id: access.peer_id.clone(),
                service_id: access.service_id.clone(),
                service_name: access.service_name.clone(),
                endpoints: access
                    .endpoints
                    .iter()
                    .map(|endpoint| LocalAccessEndpointView {
                        name: endpoint.name.clone(),
                        local_address: format!("{}:{}", endpoint.local_host, endpoint.local_port),
                    })
                    .collect(),
            })
            .collect::<Vec<_>>();

        print_json(&rows, "access list")
    }
}

fn print_access(access: &ServiceAccess, verbose: bool) {
    if verbose {
        let view = AccessInspectVerboseView {
            peer_id: access.peer_id.clone(),
            service_id: access.service_id.clone(),
            service_name: access.service_name.clone(),
            endpoints: access
                .endpoints
                .iter()
                .map(|endpoint| AccessEndpointVerboseView {
                    name: endpoint.name.clone(),
                    protocol: endpoint.protocol.clone(),
                    local_host: endpoint.local_host.clone(),
                    local_port: endpoint.local_port,
                })
                .collect(),
        };
        print_json(&view, "access")
    } else {
        let view = AccessInspectView {
            peer_id: access.peer_id.clone(),
            service_id: access.service_id.clone(),
            service_name: access.service_name.clone(),
            endpoints: access
                .endpoints
                .iter()
                .map(|endpoint| LocalAccessEndpointView {
                    name: endpoint.name.clone(),
                    local_address: format!("{}:{}", endpoint.local_host, endpoint.local_port),
                })
                .collect(),
        };
        print_json(&view, "access")
    }
}

fn build_local_urls(service: &CatalogService, access: &ServiceAccess) -> Vec<String> {
    let mut urls = access
        .endpoints
        .iter()
        .map(|endpoint| {
            let mut value = format!("http://{}:{}", endpoint.local_host, endpoint.local_port);
            if matches!(
                service.usage.as_ref().map(|usage| usage.kind),
                Some(ServiceExposeUsageKind::Web)
            ) && let Some(path) = service
                .usage
                .as_ref()
                .and_then(|usage| usage.path.as_deref())
                && !path.is_empty()
            {
                value.push_str(path);
            }
            value
        })
        .collect::<Vec<_>>();
    urls.sort();
    urls.dedup();
    urls
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

fn print_json<T: Serialize>(value: &T, label: &str) {
    match serde_json::to_string_pretty(value) {
        Ok(pretty) => println!("{pretty}"),
        Err(error) => fatal(format!("Failed to format {label}: {error}")),
    }
}

#[derive(Debug, Serialize)]
struct AccessListEntry {
    peer_id: String,
    service_id: String,
    service_name: String,
    endpoints: Vec<LocalAccessEndpointView>,
}

#[derive(Debug, Serialize)]
struct AccessListVerboseEntry {
    peer_id: String,
    service_id: String,
    service_name: String,
    endpoints: Vec<AccessEndpointVerboseView>,
}

#[derive(Debug, Serialize)]
struct LocalAccessEndpointView {
    name: String,
    local_address: String,
}

#[derive(Debug, Serialize)]
struct AccessInspectView {
    peer_id: String,
    service_id: String,
    service_name: String,
    endpoints: Vec<LocalAccessEndpointView>,
}

#[derive(Debug, Serialize)]
struct AccessInspectVerboseView {
    peer_id: String,
    service_id: String,
    service_name: String,
    endpoints: Vec<AccessEndpointVerboseView>,
}

#[derive(Debug, Serialize)]
struct AccessEndpointVerboseView {
    name: String,
    protocol: String,
    local_host: String,
    local_port: u16,
}
