use clap::Subcommand;
use fungi_daemon::{CatalogService, ServiceAccess, ServiceExposeUsageKind};
use fungi_daemon_grpc::{
    Request,
    fungi_daemon_grpc::{ListPeerCatalogRequest, ListServiceAccessesRequest},
};
use serde::Serialize;

use crate::commands::CommonArgs;

use super::{
    client::get_rpc_client,
    shared::{PeerTargetArg, fatal, fatal_grpc, print_target_peer, resolve_required_peer},
};

#[derive(Subcommand, Debug, Clone)]
pub enum CatalogCommands {
    /// List services published by a remote peer for consumption
    List {
        #[command(flatten)]
        peer: PeerTargetArg,
        /// Show detailed output
        #[arg(short, long, default_value_t = false)]
        verbose: bool,
    },
    /// Inspect one published service from a remote peer
    Inspect {
        /// Published service identifier
        service_id: String,
        #[command(flatten)]
        peer: PeerTargetArg,
        /// Show detailed output
        #[arg(short, long, default_value_t = false)]
        verbose: bool,
    },
}

pub async fn execute_catalog(args: CommonArgs, cmd: CatalogCommands) {
    let mut client = match get_rpc_client(&args).await {
        Some(c) => c,
        None => fatal("Cannot connect to Fungi daemon. Is it running?"),
    };

    match cmd {
        CatalogCommands::List { peer, verbose } => {
            let peer = match resolve_required_peer(&args, peer.peer.as_ref()) {
                Ok(peer) => peer,
                Err(error) => fatal(error),
            };
            print_target_peer(&peer);
            let services = discover_services(&mut client, &peer.peer_id).await;
            let attached = list_attached_services(&mut client, &peer.peer_id).await;
            print_catalog_services(&peer.peer_id, &services, &attached, verbose);
        }
        CatalogCommands::Inspect {
            service_id,
            peer,
            verbose,
        } => {
            let peer = match resolve_required_peer(&args, peer.peer.as_ref()) {
                Ok(peer) => peer,
                Err(error) => fatal(error),
            };
            print_target_peer(&peer);
            let services = discover_services(&mut client, &peer.peer_id).await;
            let attached = list_attached_services(&mut client, &peer.peer_id).await;
            let Some(service) = services
                .into_iter()
                .find(|service| service.service_id == service_id)
            else {
                fatal(format!(
                    "Published service not found on peer {}: {}",
                    peer.peer_id, service_id
                ));
            };
            print_catalog_service(&peer.peer_id, service, &attached, verbose);
        }
    }
}

fn print_catalog_services(
    peer_id: &str,
    services: &[CatalogService],
    attached: &[ServiceAccess],
    verbose: bool,
) {
    let attached_by_service = attached
        .iter()
        .map(|service| (service.service_id.as_str(), service))
        .collect::<std::collections::BTreeMap<_, _>>();

    if verbose {
        let rows = services
            .iter()
            .map(|service| CatalogListVerboseEntry {
                peer_id: peer_id.to_string(),
                service_id: service.service_id.clone(),
                display_name: service.display_name.clone(),
                service_name: service.service_name.clone(),
                usage: catalog_usage_label(service),
                runtime: service.runtime,
                transport: catalog_transport_label(service),
                attached: attached_by_service.contains_key(service.service_id.as_str()),
                local_urls: attached_by_service
                    .get(service.service_id.as_str())
                    .map(|access| build_local_urls(service, access))
                    .unwrap_or_default(),
                endpoints: service
                    .endpoints
                    .iter()
                    .map(|endpoint| CatalogVerboseEndpointView {
                        name: endpoint.name.clone(),
                        protocol: endpoint.protocol.clone(),
                        service_port: endpoint.service_port,
                    })
                    .collect(),
            })
            .collect::<Vec<_>>();
        print_json(&rows, "catalog list")
    } else {
        let rows = services
            .iter()
            .map(|service| CatalogListEntry {
                service_id: service.service_id.clone(),
                display_name: service.display_name.clone(),
                usage: catalog_usage_label(service),
                attached: attached_by_service.contains_key(service.service_id.as_str()),
                local_urls: attached_by_service
                    .get(service.service_id.as_str())
                    .map(|access| build_local_urls(service, access))
                    .unwrap_or_default(),
            })
            .collect::<Vec<_>>();

        print_json(&rows, "catalog list")
    }
}

fn print_catalog_service(
    peer_id: &str,
    service: CatalogService,
    attached: &[ServiceAccess],
    verbose: bool,
) {
    let attached_access = attached
        .iter()
        .find(|access| access.service_id == service.service_id);
    let usage = catalog_usage_label(&service);
    let local_urls = attached_access
        .map(|access| build_local_urls(&service, access))
        .unwrap_or_default();
    let transport = catalog_transport_label(&service);

    if verbose {
        let view = CatalogInspectVerboseView {
            peer_id: peer_id.to_string(),
            service_id: service.service_id.clone(),
            display_name: service.display_name.clone(),
            service_name: service.service_name,
            usage,
            runtime: service.runtime,
            transport,
            status: service.status.state,
            running: service.status.running,
            attached: attached_access.is_some(),
            local_urls,
            icon_url: service.icon_url,
            catalog_id: service.catalog_id,
            endpoints: service
                .endpoints
                .into_iter()
                .map(|endpoint| CatalogVerboseEndpointView {
                    name: endpoint.name,
                    protocol: endpoint.protocol,
                    service_port: endpoint.service_port,
                })
                .collect(),
        };

        print_json(&view, "catalog inspect")
    } else {
        let view = CatalogInspectView {
            service_id: service.service_id.clone(),
            display_name: service.display_name.clone(),
            service_name: service.service_name,
            usage,
            status: service.status.state,
            running: service.status.running,
            attached: attached_access.is_some(),
            local_urls,
            endpoints: service
                .endpoints
                .into_iter()
                .map(|endpoint| CatalogEndpointView {
                    name: endpoint.name,
                    protocol: endpoint.protocol,
                })
                .collect(),
        };

        print_json(&view, "catalog inspect")
    }
}

async fn discover_services(
    client: &mut fungi_daemon_grpc::fungi_daemon_grpc::fungi_daemon_client::FungiDaemonClient<
        tonic::transport::Channel,
    >,
    peer_id: &str,
) -> Vec<CatalogService> {
    let req = ListPeerCatalogRequest {
        peer_id: peer_id.to_string(),
        cached: false,
    };
    match client.list_peer_catalog(Request::new(req)).await {
        Ok(resp) => {
            match serde_json::from_str::<Vec<CatalogService>>(&resp.into_inner().services_json) {
                Ok(services) => services,
                Err(error) => fatal(format!("Failed to decode catalog services: {error}")),
            }
        }
        Err(error) => fatal_grpc(error),
    }
}

async fn list_attached_services(
    client: &mut fungi_daemon_grpc::fungi_daemon_grpc::fungi_daemon_client::FungiDaemonClient<
        tonic::transport::Channel,
    >,
    peer_id: &str,
) -> Vec<ServiceAccess> {
    let req = ListServiceAccessesRequest {
        peer_id: peer_id.to_string(),
    };
    match client.list_service_accesses(Request::new(req)).await {
        Ok(resp) => match serde_json::from_str::<Vec<ServiceAccess>>(
            &resp.into_inner().service_accesses_json,
        ) {
            Ok(services) => services,
            Err(error) => fatal(format!("Failed to decode attached access list: {error}")),
        },
        Err(error) => fatal_grpc(error),
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

fn catalog_usage_label(service: &CatalogService) -> String {
    service
        .usage
        .as_ref()
        .map(|usage| match usage.kind {
            ServiceExposeUsageKind::Web => "web".to_string(),
            ServiceExposeUsageKind::Ssh => "ssh".to_string(),
            ServiceExposeUsageKind::Raw => "raw".to_string(),
        })
        .unwrap_or_else(|| "unknown".to_string())
}

fn catalog_transport_label(service: &CatalogService) -> String {
    match service.transport.kind {
        fungi_daemon::ServiceExposeTransportKind::Tcp => "tcp".to_string(),
        fungi_daemon::ServiceExposeTransportKind::Raw => "raw".to_string(),
    }
}

fn print_json<T: Serialize>(value: &T, label: &str) {
    match serde_json::to_string_pretty(value) {
        Ok(pretty) => println!("{pretty}"),
        Err(error) => fatal(format!("Failed to format {label}: {error}")),
    }
}

#[derive(Debug, Serialize)]
struct CatalogListEntry {
    service_id: String,
    display_name: String,
    usage: String,
    attached: bool,
    local_urls: Vec<String>,
}

#[derive(Debug, Serialize)]
struct CatalogListVerboseEntry {
    peer_id: String,
    service_id: String,
    display_name: String,
    service_name: String,
    usage: String,
    runtime: fungi_daemon::RuntimeKind,
    transport: String,
    attached: bool,
    local_urls: Vec<String>,
    endpoints: Vec<CatalogVerboseEndpointView>,
}

#[derive(Debug, Serialize)]
struct CatalogInspectView {
    service_id: String,
    display_name: String,
    service_name: String,
    usage: String,
    status: String,
    running: bool,
    attached: bool,
    local_urls: Vec<String>,
    endpoints: Vec<CatalogEndpointView>,
}

#[derive(Debug, Serialize)]
struct CatalogInspectVerboseView {
    peer_id: String,
    service_id: String,
    display_name: String,
    service_name: String,
    usage: String,
    runtime: fungi_daemon::RuntimeKind,
    transport: String,
    status: String,
    running: bool,
    attached: bool,
    local_urls: Vec<String>,
    icon_url: Option<String>,
    catalog_id: Option<String>,
    endpoints: Vec<CatalogVerboseEndpointView>,
}

#[derive(Debug, Serialize)]
struct CatalogEndpointView {
    name: String,
    protocol: String,
}

#[derive(Debug, Serialize)]
struct CatalogVerboseEndpointView {
    name: String,
    protocol: String,
    service_port: u16,
}
