use clap::Subcommand;
use fungi_daemon_grpc::{
    Request,
    fungi_daemon_grpc::{
        AddIncomingAllowedPeerRequest, DeviceInfo, Empty, GetDeviceRequest,
        RemoveIncomingAllowedPeerRequest, RuntimeConfigResponse, UpdateDeviceRequest,
    },
};
use libp2p::PeerId;
use std::io::{self, Write};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::commands::CommonArgs;

use super::{
    client::get_rpc_client,
    shared::{fatal, fatal_grpc, host_path_risk_note, resolve_peer_value},
};

#[derive(Subcommand, Debug, Clone)]
pub enum AllowedPeerCommands {
    /// List peers allowed to initiate incoming connections
    List,
    /// Add a peer to the incoming connection allowlist
    Add {
        /// Peer ID or device name. For an unnamed peer ID, pass --name to save it.
        peer: String,
        /// Device name to add or update
        #[arg(long)]
        name: Option<String>,
    },
    /// Remove a peer from the incoming connection allowlist
    Remove {
        /// Peer ID or device name to remove
        peer: String,
    },
}

pub async fn execute_allowed_peer(args: CommonArgs, cmd: AllowedPeerCommands) {
    let mut client = match get_rpc_client(&args).await {
        Some(c) => c,
        None => fatal("Cannot connect to Fungi daemon. Is it running?"),
    };

    match cmd {
        AllowedPeerCommands::List => {
            match client
                .get_incoming_allowed_peers(Request::new(Empty {}))
                .await
            {
                Ok(resp) => {
                    let peers = resp.into_inner().peers;
                    if peers.is_empty() {
                        println!("No allowed peers");
                    } else {
                        for peer in peers {
                            println!("{} - {}", peer.peer_id, peer.name);
                        }
                    }
                }
                Err(e) => fatal_grpc(e),
            }
        }
        AllowedPeerCommands::Add { peer, name } => {
            let resolved = resolve_allowed_peer_for_add(&args, &mut client, &peer, name).await;
            let runtime_config = get_runtime_config(&mut client).await;

            if !confirm_allowed_peer_add(&resolved, &runtime_config) {
                println!("Aborted. No changes were made.");
                return;
            }

            if resolved.devices_needs_update {
                let name = resolved
                    .name
                    .clone()
                    .unwrap_or_else(|| fatal("Missing name for device update"));
                let existing = get_device(&mut client, &resolved.peer_id).await;
                upsert_device(&mut client, existing, &resolved.peer_id, name).await;
            }

            let req = AddIncomingAllowedPeerRequest {
                peer_id: resolved.peer_id.clone(),
            };
            match client.add_incoming_allowed_peer(Request::new(req)).await {
                Ok(_) => {
                    if resolved.devices_needs_update {
                        let name = resolved.name.as_deref().unwrap_or("<unnamed>");
                        println!("Device updated: {} -> {}", resolved.peer_id, name);
                    }
                    println!("Peer added successfully");
                    print_allowed_peer_warning(&resolved);
                }
                Err(e) => fatal_grpc(e),
            }
        }
        AllowedPeerCommands::Remove { peer } => {
            let peer_id = match resolve_peer_value(&args, &peer) {
                Ok(peer) => peer.peer_id,
                Err(_) => peer,
            };
            let req = RemoveIncomingAllowedPeerRequest { peer_id };
            match client.remove_incoming_allowed_peer(Request::new(req)).await {
                Ok(_) => println!("Peer removed successfully"),
                Err(e) => fatal_grpc(e),
            }
        }
    }
}

#[derive(Debug, Clone)]
struct AllowedPeerTarget {
    peer_id: String,
    name: Option<String>,
    devices_needs_update: bool,
}

async fn resolve_allowed_peer_for_add(
    args: &CommonArgs,
    client: &mut fungi_daemon_grpc::fungi_daemon_grpc::fungi_daemon_client::FungiDaemonClient<
        tonic::transport::Channel,
    >,
    peer: &str,
    name: Option<String>,
) -> AllowedPeerTarget {
    let requested_name = name
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    if let Ok(peer_id) = peer.parse::<PeerId>() {
        let peer_id = peer_id.to_string();
        let existing = get_device(client, &peer_id).await;
        let devices_needs_update = requested_name.is_some();

        let name = match requested_name {
            Some(name) => Some(name),
            None => existing
                .as_ref()
                .map(|info| info.name.trim().to_string())
                .filter(|value| !value.is_empty())
                .or_else(|| {
                    fatal(format!(
                        "Peer {} is not named yet. Re-run with `--name <name>` to save it as a device and allow it in one step.",
                        peer_id
                    ))
                }),
        };

        return AllowedPeerTarget {
            peer_id,
            name,
            devices_needs_update,
        };
    }

    let resolved = match resolve_peer_value(args, peer) {
        Ok(peer) => peer,
        Err(error) => fatal(error),
    };

    if let Some(name) = requested_name {
        return AllowedPeerTarget {
            peer_id: resolved.peer_id,
            name: Some(name),
            devices_needs_update: true,
        };
    }

    AllowedPeerTarget {
        peer_id: resolved.peer_id,
        name: resolved.name,
        devices_needs_update: false,
    }
}

async fn get_runtime_config(
    client: &mut fungi_daemon_grpc::fungi_daemon_grpc::fungi_daemon_client::FungiDaemonClient<
        tonic::transport::Channel,
    >,
) -> RuntimeConfigResponse {
    match client.get_runtime_config(Request::new(Empty {})).await {
        Ok(resp) => resp.into_inner(),
        Err(error) => fatal_grpc(error),
    }
}

async fn get_device(
    client: &mut fungi_daemon_grpc::fungi_daemon_grpc::fungi_daemon_client::FungiDaemonClient<
        tonic::transport::Channel,
    >,
    peer_id: &str,
) -> Option<DeviceInfo> {
    match client
        .get_device(Request::new(GetDeviceRequest {
            peer_id: peer_id.to_string(),
        }))
        .await
    {
        Ok(resp) => resp.into_inner().device,
        Err(error) => fatal_grpc(error),
    }
}

async fn upsert_device(
    client: &mut fungi_daemon_grpc::fungi_daemon_grpc::fungi_daemon_client::FungiDaemonClient<
        tonic::transport::Channel,
    >,
    existing: Option<DeviceInfo>,
    peer_id: &str,
    name: String,
) {
    let device_info = match existing {
        Some(mut device_info) => {
            device_info.name = name;
            device_info
        }
        None => new_minimal_device_info(peer_id.to_string(), name),
    };

    match client
        .update_device(Request::new(UpdateDeviceRequest {
            device: Some(device_info),
        }))
        .await
    {
        Ok(_) => {}
        Err(error) => fatal_grpc(error),
    }
}

fn new_minimal_device_info(peer_id: String, name: String) -> DeviceInfo {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    DeviceInfo {
        peer_id,
        name,
        hostname: String::new(),
        os: "Unknown".to_string(),
        public_ip: String::new(),
        private_ips: Vec::new(),
        created_at: now,
        last_connected: now,
        version: String::new(),
        multiaddrs: Vec::new(),
    }
}

fn print_allowed_peer_warning(peer: &AllowedPeerTarget) {
    let display_name = peer.name.as_deref().unwrap_or(&peer.peer_id);

    println!();
    println!("================ IMPORTANT SECURITY NOTICE ================");
    println!(
        "{} is now allowed to access host paths configured via `security allow-path`.",
        display_name
    );
    println!("{} can also manage services on this device.", display_name);
    println!("Peer ID: {}", peer.peer_id);
    println!("===========================================================");
}

fn confirm_allowed_peer_add(peer: &AllowedPeerTarget, config: &RuntimeConfigResponse) -> bool {
    let display_name = peer.name.as_deref().unwrap_or(&peer.peer_id);

    println!();
    println!("================ SECURITY CONFIRMATION ================");
    println!("You are about to trust this peer for incoming access:");
    println!("  Peer: {}", display_name);
    println!("  Peer ID: {}", peer.peer_id);
    if peer.devices_needs_update {
        println!("  Address book: will be added or updated");
    }
    println!();
    println!("This peer will be able to:");
    println!("  1. Access these allowed host paths:");
    print_string_list(&config.allowed_host_paths, "    - none configured");
    println!("  2. Manage services on this device");
    println!("=======================================================");

    prompt_yes_no("Proceed? [Y/n]: ")
}

fn prompt_yes_no(prompt: &str) -> bool {
    print!("{prompt}");
    if io::stdout().flush().is_err() {
        fatal("Failed to flush confirmation prompt")
    }

    let mut input = String::new();
    match io::stdin().read_line(&mut input) {
        Ok(_) => match input.trim().to_ascii_lowercase().as_str() {
            "" | "y" | "yes" => true,
            "n" | "no" => false,
            other => fatal(format!(
                "Invalid confirmation response: {other}. Expected Y, y, N, n, yes, no, or Enter."
            )),
        },
        Err(error) => fatal(format!("Failed to read confirmation response: {error}")),
    }
}

fn print_string_list(items: &[String], empty_message: &str) {
    if items.is_empty() {
        println!("{empty_message}");
        return;
    }

    for item in items {
        println!("    - {item}");
        if let Some(note) = host_path_risk_note(item) {
            println!("      ! {note}");
        }
    }
}
