use clap::{Args, Subcommand};
use fungi_daemon_grpc::{
    Request,
    fungi_daemon_grpc::{
        DeviceInfo, Empty, GetDeviceRequest, RemoveDeviceRequest, UpdateDeviceRequest,
    },
};
use libp2p::PeerId;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::commands::CommonArgs;

use super::{
    TrustedDeviceCommands,
    client::get_rpc_client,
    execute_trusted_device,
    shared::{fatal, fatal_grpc, resolve_peer_value},
};

#[derive(Args, Debug, Clone)]
pub struct DeviceArgs {
    #[command(subcommand)]
    pub command: Option<DeviceCommands>,
}

#[derive(Subcommand, Debug, Clone)]
pub enum DeviceCommands {
    /// List devices discovered via mDNS
    Mdns,
    /// List saved devices
    List,
    /// Add a device with a required device name
    Add {
        /// Human-friendly unique device name
        #[arg(value_name = "NAME")]
        name: String,
        /// Device ID to add
        #[arg(value_name = "DEVICE_ID")]
        peer_id: String,
        /// User-managed direct multiaddr for this device
        #[arg(long = "addr", alias = "address", value_name = "MULTIADDR")]
        addresses: Vec<String>,
    },
    /// Manage user-added device addresses
    #[command(subcommand)]
    Address(DeviceAddressCommands),
    /// List devices trusted to initiate incoming access
    Trusted,
    /// Trust a saved device for incoming access
    Trust {
        /// Device name or device ID
        device: String,
    },
    /// Remove incoming access trust from a device
    Untrust {
        /// Device name or device ID
        device: String,
    },
    /// Rename an existing device
    Rename {
        /// Device ID or device name to rename
        peer: String,
        /// New human-friendly unique device name
        #[arg(value_name = "NAME")]
        name: String,
    },
    /// Get information about a specific device
    Get {
        /// Device name or device ID to query
        device: String,
    },
    /// Remove a device
    Remove {
        /// Device name or device ID to remove
        device: String,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum DeviceAddressCommands {
    /// List user-managed addresses for a device
    List {
        /// Device ID or device name
        device: String,
    },
    /// Add a user-managed address to a device
    Add {
        /// Device ID or device name
        device: String,
        /// Direct multiaddr to add
        address: String,
    },
    /// Remove a user-managed address from a device
    Remove {
        /// Device ID or device name
        device: String,
        /// Direct multiaddr to remove
        address: String,
    },
}

pub async fn execute_device(args: CommonArgs, device_args: DeviceArgs) {
    let cmd = device_args.command.unwrap_or(DeviceCommands::List);

    match &cmd {
        DeviceCommands::Trusted => {
            execute_trusted_device(args, TrustedDeviceCommands::List).await;
            return;
        }
        DeviceCommands::Trust { device } => {
            execute_trusted_device(
                args,
                TrustedDeviceCommands::Trust {
                    device: device.clone(),
                },
            )
            .await;
            return;
        }
        DeviceCommands::Untrust { device } => {
            execute_trusted_device(
                args,
                TrustedDeviceCommands::Untrust {
                    device: device.clone(),
                },
            )
            .await;
            return;
        }
        _ => {}
    }

    let mut client = match get_rpc_client(&args).await {
        Some(c) => c,
        None => fatal("Cannot connect to Fungi daemon. Is it running?"),
    };

    match cmd {
        DeviceCommands::Mdns => match client.list_mdns_devices(Request::new(Empty {})).await {
            Ok(resp) => {
                let devices = resp.into_inner().devices;
                if devices.is_empty() {
                    println!("No devices discovered");
                } else {
                    for device in devices {
                        print_device_info(&device);
                    }
                }
            }
            Err(e) => fatal_grpc(e),
        },
        DeviceCommands::List => match client.list_devices(Request::new(Empty {})).await {
            Ok(resp) => {
                let devices = resp.into_inner().devices;
                if devices.is_empty() {
                    println!("No devices saved");
                } else {
                    for device in devices {
                        print_device_info(&device);
                    }
                }
            }
            Err(e) => fatal_grpc(e),
        },
        DeviceCommands::Add {
            peer_id,
            name,
            addresses,
        } => {
            let peer_id = match peer_id.parse::<PeerId>() {
                Ok(value) => value,
                Err(error) => fatal(format!("Invalid device ID: {error}")),
            };
            let addresses = normalize_multiaddrs(addresses);

            let existing = match client
                .get_device(Request::new(GetDeviceRequest {
                    peer_id: peer_id.to_string(),
                }))
                .await
            {
                Ok(resp) => resp.into_inner().device,
                Err(error) => fatal_grpc(error),
            };

            let device_info = match existing {
                Some(mut device) => {
                    device.name = name;
                    device.multiaddrs = merge_multiaddrs(device.multiaddrs, addresses);
                    device
                }
                None => new_minimal_device_info(peer_id.to_string(), name, addresses),
            };

            match client
                .update_device(Request::new(UpdateDeviceRequest {
                    device: Some(device_info),
                }))
                .await
            {
                Ok(_) => println!("Device saved"),
                Err(error) => fatal_grpc(error),
            }
        }
        DeviceCommands::Address(command) => match command {
            DeviceAddressCommands::List { device } => {
                let peer = get_saved_device(&args, &mut client, &device).await;
                if peer.multiaddrs.is_empty() {
                    println!("No manual addresses");
                } else {
                    for address in peer.multiaddrs {
                        println!("{address}");
                    }
                }
            }
            DeviceAddressCommands::Add { device, address } => {
                let mut peer = get_saved_device(&args, &mut client, &device).await;
                peer.multiaddrs =
                    merge_multiaddrs(peer.multiaddrs, normalize_multiaddrs(vec![address]));
                save_device(&mut client, peer, "Device address added").await;
            }
            DeviceAddressCommands::Remove { device, address } => {
                let mut peer = get_saved_device(&args, &mut client, &device).await;
                let normalized = normalize_multiaddr(&address);
                let before = peer.multiaddrs.len();
                peer.multiaddrs.retain(|value| value != &normalized);
                if peer.multiaddrs.len() == before {
                    fatal("Device address not found")
                }
                save_device(&mut client, peer, "Device address removed").await;
            }
        },
        DeviceCommands::Trusted | DeviceCommands::Trust { .. } | DeviceCommands::Untrust { .. } => {
            unreachable!()
        }
        DeviceCommands::Rename { peer, name } => {
            let target_peer_id = if let Ok(value) = peer.parse::<PeerId>() {
                value.to_string()
            } else {
                match resolve_peer_value(&args, &peer) {
                    Ok(peer) => peer.peer_id,
                    Err(error) => fatal(error),
                }
            };

            let device_info = match client
                .get_device(Request::new(GetDeviceRequest {
                    peer_id: target_peer_id,
                }))
                .await
            {
                Ok(resp) => match resp.into_inner().device {
                    Some(mut device) => {
                        device.name = name;
                        device
                    }
                    None => fatal("Device not found"),
                },
                Err(error) => fatal_grpc(error),
            };

            match client
                .update_device(Request::new(UpdateDeviceRequest {
                    device: Some(device_info),
                }))
                .await
            {
                Ok(_) => println!("Device name updated"),
                Err(error) => fatal_grpc(error),
            }
        }
        DeviceCommands::Get { device } => {
            let peer_id = match resolve_peer_value(&args, &device) {
                Ok(peer) => peer.peer_id,
                Err(error) => fatal(error),
            };
            let req = GetDeviceRequest { peer_id };
            match client.get_device(Request::new(req)).await {
                Ok(resp) => {
                    if let Some(device) = resp.into_inner().device {
                        print_device_info_detailed(&device);
                    } else {
                        println!("Device not found");
                    }
                }
                Err(e) => fatal_grpc(e),
            }
        }
        DeviceCommands::Remove { device } => {
            let peer_id = match resolve_peer_value(&args, &device) {
                Ok(peer) => peer.peer_id,
                Err(error) => fatal(error),
            };
            let req = RemoveDeviceRequest { peer_id };
            match client.remove_device(Request::new(req)).await {
                Ok(_) => println!("Device removed successfully"),
                Err(e) => fatal_grpc(e),
            }
        }
    }
}

async fn get_saved_device(
    args: &CommonArgs,
    client: &mut fungi_daemon_grpc::fungi_daemon_grpc::fungi_daemon_client::FungiDaemonClient<
        tonic::transport::Channel,
    >,
    device: &str,
) -> DeviceInfo {
    let peer_id = match resolve_peer_value(args, device) {
        Ok(peer) => peer.peer_id,
        Err(error) => fatal(error),
    };

    match client
        .get_device(Request::new(GetDeviceRequest { peer_id }))
        .await
    {
        Ok(resp) => resp
            .into_inner()
            .device
            .unwrap_or_else(|| fatal("Device not found")),
        Err(error) => fatal_grpc(error),
    }
}

async fn save_device(
    client: &mut fungi_daemon_grpc::fungi_daemon_grpc::fungi_daemon_client::FungiDaemonClient<
        tonic::transport::Channel,
    >,
    device_info: DeviceInfo,
    message: &str,
) {
    match client
        .update_device(Request::new(UpdateDeviceRequest {
            device: Some(device_info),
        }))
        .await
    {
        Ok(_) => println!("{message}"),
        Err(error) => fatal_grpc(error),
    }
}

fn new_minimal_device_info(peer_id: String, name: String, multiaddrs: Vec<String>) -> DeviceInfo {
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
        multiaddrs,
    }
}

fn normalize_multiaddrs(addresses: Vec<String>) -> Vec<String> {
    let mut addresses = addresses
        .into_iter()
        .map(|address| normalize_multiaddr(&address))
        .collect::<Vec<_>>();
    addresses.sort();
    addresses.dedup();
    addresses
}

fn normalize_multiaddr(address: &str) -> String {
    let address = address.trim();
    if address.is_empty() {
        fatal("Device address cannot be empty")
    }
    if let Err(error) = address.parse::<multiaddr::Multiaddr>() {
        fatal(format!("Invalid multiaddr: {error}"))
    }
    address.to_string()
}

fn merge_multiaddrs(existing: Vec<String>, added: Vec<String>) -> Vec<String> {
    let mut merged = existing
        .into_iter()
        .map(|address| normalize_multiaddr(&address))
        .chain(added)
        .collect::<Vec<_>>();
    merged.sort();
    merged.dedup();
    merged
}

fn print_device_info(device: &DeviceInfo) {
    println!("{} - {} ({})", device.peer_id, device.name, device.hostname);
}

fn print_device_info_detailed(device: &DeviceInfo) {
    println!("Device ID: {}", device.peer_id);
    println!("Device name: {}", device.name);
    println!("Hostname: {}", device.hostname);
    println!("OS: {}", device.os);
    println!("Version: {}", device.version);
    if !device.public_ip.is_empty() {
        println!("Public IP: {}", device.public_ip);
    }
    if !device.private_ips.is_empty() {
        println!("Private IPs: {}", device.private_ips.join(", "));
    }
    if !device.multiaddrs.is_empty() {
        println!("Manual addresses:");
        for address in &device.multiaddrs {
            println!("  {address}");
        }
    }
}
