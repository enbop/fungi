use clap::Subcommand;
use fungi_daemon_grpc::{
    Request,
    fungi_daemon_grpc::{Empty, RuntimeConfigResponse, TrustDeviceRequest, UntrustDeviceRequest},
};
use std::io::{self, Write};

use crate::commands::CommonArgs;

use super::{
    client::get_rpc_client,
    shared::{ResolvedPeerTarget, fatal, fatal_grpc, host_path_risk_note, resolve_peer_value},
};

#[derive(Subcommand, Debug, Clone)]
pub enum TrustedDeviceCommands {
    /// List devices trusted to initiate incoming access
    List,
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
}

pub async fn execute_trusted_device(args: CommonArgs, cmd: TrustedDeviceCommands) {
    let mut client = match get_rpc_client(&args).await {
        Some(c) => c,
        None => fatal("Cannot connect to Fungi daemon. Is it running?"),
    };

    match cmd {
        TrustedDeviceCommands::List => {
            match client.list_trusted_devices(Request::new(Empty {})).await {
                Ok(resp) => {
                    let devices = resp.into_inner().devices;
                    if devices.is_empty() {
                        println!("No trusted devices");
                    } else {
                        for device in devices {
                            println!("{} - {}", device.peer_id, device.name);
                        }
                    }
                }
                Err(e) => fatal_grpc(e),
            }
        }
        TrustedDeviceCommands::Trust { device } => {
            let resolved = match resolve_peer_value(&args, &device) {
                Ok(device) => device,
                Err(error) => fatal(error),
            };
            let runtime_config = get_runtime_config(&mut client).await;

            if !confirm_device_trust(&resolved, &runtime_config) {
                println!("Aborted. No changes were made.");
                return;
            }

            let req = TrustDeviceRequest {
                peer_id: resolved.peer_id.clone(),
            };
            match client.trust_device(Request::new(req)).await {
                Ok(_) => {
                    println!("Device trusted");
                    print_trusted_device_warning(&resolved);
                }
                Err(e) => fatal_grpc(e),
            }
        }
        TrustedDeviceCommands::Untrust { device } => {
            let resolved = match resolve_peer_value(&args, &device) {
                Ok(device) => device,
                Err(error) => fatal(error),
            };
            let req = UntrustDeviceRequest {
                peer_id: resolved.peer_id,
            };
            match client.untrust_device(Request::new(req)).await {
                Ok(_) => println!("Device untrusted"),
                Err(e) => fatal_grpc(e),
            }
        }
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

fn print_trusted_device_warning(device: &ResolvedPeerTarget) {
    let display_name = device.name.as_deref().unwrap_or(&device.peer_id);

    println!();
    println!("================ IMPORTANT SECURITY NOTICE ================");
    println!(
        "{} is now allowed to access host paths configured via `security allow-path`.",
        display_name
    );
    println!("{} can also manage services on this device.", display_name);
    println!("Device ID: {}", device.peer_id);
    println!("===========================================================");
}

fn confirm_device_trust(device: &ResolvedPeerTarget, config: &RuntimeConfigResponse) -> bool {
    let display_name = device.name.as_deref().unwrap_or(&device.peer_id);

    println!();
    println!("================ SECURITY CONFIRMATION ================");
    println!("You are about to trust this device for incoming access:");
    println!("  Device: {}", display_name);
    println!("  Device ID: {}", device.peer_id);
    println!();
    println!("This device will be able to:");
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
