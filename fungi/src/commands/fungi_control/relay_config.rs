use clap::{Subcommand, ValueEnum};
use fungi_config::{FungiConfig, FungiDir};
use fungi_daemon_grpc::{
    Request,
    fungi_daemon_grpc::{
        Empty, RelayAddressRequest, RelayConfigResponse, RelayEnabledRequest,
        UseCommunityRelaysRequest,
    },
};

use crate::commands::CommonArgs;

use super::{
    client::get_rpc_client,
    shared::{fatal, fatal_grpc},
};

#[derive(Subcommand, Debug, Clone)]
pub enum RelayCommands {
    /// Show current relay configuration
    Show,
    /// Enable relay usage
    Enable,
    /// Disable relay usage
    Disable,
    /// Enable or disable built-in community relays
    UseCommunity {
        #[arg(value_enum)]
        mode: RelayMode,
    },
    /// Add a custom relay multiaddr
    Add { address: String },
    /// Remove a custom relay multiaddr
    Remove { address: String },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum RelayMode {
    On,
    Off,
}

pub async fn execute_relay(args: CommonArgs, cmd: RelayCommands) {
    fungi_config::init(&args, false)
        .unwrap_or_else(|error| fatal(format!("Failed to initialize config: {error}")));

    let mut client = get_rpc_client(&args).await;

    match cmd {
        RelayCommands::Show => {
            if let Some(client) = client.as_mut() {
                match client.get_relay_config(Request::new(Empty {})).await {
                    Ok(resp) => print_proto_relay_config(resp.into_inner()),
                    Err(error) => fatal_grpc(error),
                }
            } else {
                print_local_relay_config(&read_config(&args));
            }
        }
        RelayCommands::Enable => {
            if let Some(client) = client.as_mut() {
                match client
                    .set_relay_enabled(Request::new(RelayEnabledRequest { enabled: true }))
                    .await
                {
                    Ok(_) => print_update_message("Relay enabled", true),
                    Err(error) => fatal_grpc(error),
                }
            } else {
                let config = read_config(&args);
                config.set_relay_enabled(true).unwrap_or_else(|error| {
                    fatal(format!("Failed to update relay config: {error}"))
                });
                print_update_message("Relay enabled", false);
            }
        }
        RelayCommands::Disable => {
            if let Some(client) = client.as_mut() {
                match client
                    .set_relay_enabled(Request::new(RelayEnabledRequest { enabled: false }))
                    .await
                {
                    Ok(_) => print_update_message("Relay disabled", true),
                    Err(error) => fatal_grpc(error),
                }
            } else {
                let config = read_config(&args);
                config.set_relay_enabled(false).unwrap_or_else(|error| {
                    fatal(format!("Failed to update relay config: {error}"))
                });
                print_update_message("Relay disabled", false);
            }
        }
        RelayCommands::UseCommunity { mode } => {
            let enabled = matches!(mode, RelayMode::On);
            if let Some(client) = client.as_mut() {
                match client
                    .set_use_community_relays(Request::new(UseCommunityRelaysRequest { enabled }))
                    .await
                {
                    Ok(_) => print_update_message(
                        if enabled {
                            "Community relay enabled"
                        } else {
                            "Community relay disabled"
                        },
                        true,
                    ),
                    Err(error) => fatal_grpc(error),
                }
            } else {
                let config = read_config(&args);
                config
                    .set_use_community_relays(enabled)
                    .unwrap_or_else(|error| {
                        fatal(format!("Failed to update relay config: {error}"))
                    });
                print_update_message(
                    if enabled {
                        "Community relay enabled"
                    } else {
                        "Community relay disabled"
                    },
                    false,
                );
            }
        }
        RelayCommands::Add { address } => {
            validate_multiaddr(&address);
            if let Some(client) = client.as_mut() {
                match client
                    .add_custom_relay_address(Request::new(RelayAddressRequest {
                        address: address.clone(),
                    }))
                    .await
                {
                    Ok(_) => print_update_message("Custom relay added", true),
                    Err(error) => fatal_grpc(error),
                }
            } else {
                let config = read_config(&args);
                let parsed = address
                    .parse()
                    .unwrap_or_else(|error| fatal(format!("Invalid relay address: {error}")));
                config
                    .add_custom_relay_address(parsed)
                    .unwrap_or_else(|error| {
                        fatal(format!("Failed to update relay config: {error}"))
                    });
                print_update_message("Custom relay added", false);
            }
        }
        RelayCommands::Remove { address } => {
            validate_multiaddr(&address);
            if let Some(client) = client.as_mut() {
                match client
                    .remove_custom_relay_address(Request::new(RelayAddressRequest {
                        address: address.clone(),
                    }))
                    .await
                {
                    Ok(_) => print_update_message("Custom relay removed", true),
                    Err(error) => fatal_grpc(error),
                }
            } else {
                let config = read_config(&args);
                let parsed = address
                    .parse()
                    .unwrap_or_else(|error| fatal(format!("Invalid relay address: {error}")));
                config
                    .remove_custom_relay_address(&parsed)
                    .unwrap_or_else(|error| {
                        fatal(format!("Failed to update relay config: {error}"))
                    });
                print_update_message("Custom relay removed", false);
            }
        }
    }
}

fn read_config(args: &CommonArgs) -> FungiConfig {
    FungiConfig::apply_from_dir(&args.fungi_dir())
        .unwrap_or_else(|error| fatal(format!("Failed to read configuration: {error}")))
}

fn validate_multiaddr(address: &str) {
    let _: libp2p::Multiaddr = address
        .parse()
        .unwrap_or_else(|error| fatal(format!("Invalid relay address: {error}")));
}

fn print_update_message(message: &str, needs_restart: bool) {
    println!("{message}");
    if needs_restart {
        println!("Relay config updated. Restart daemon to fully apply changes.");
    }
}

fn print_proto_relay_config(response: RelayConfigResponse) {
    println!("relay_enabled: {}", response.relay_enabled);
    println!("use_community_relays: {}", response.use_community_relays);
    println!("custom_relay_addresses:");
    if response.custom_relay_addresses.is_empty() {
        println!("  <none>");
    } else {
        for address in response.custom_relay_addresses {
            println!("  {}", address);
        }
    }

    println!("effective_relay_addresses:");
    if response.effective_relay_addresses.is_empty() {
        println!("  <none>");
    } else {
        for entry in response.effective_relay_addresses {
            println!("  [{}] {}", entry.source, entry.address);
        }
    }
}

fn print_local_relay_config(config: &FungiConfig) {
    let effective = config
        .network
        .effective_relay_addresses(&fungi_swarm::get_default_relay_addrs());

    println!("relay_enabled: {}", config.network.relay_enabled);
    println!(
        "use_community_relays: {}",
        config.network.use_community_relays
    );
    println!("custom_relay_addresses:");
    if config.network.custom_relay_addresses.is_empty() {
        println!("  <none>");
    } else {
        for address in &config.network.custom_relay_addresses {
            println!("  {}", address);
        }
    }

    println!("effective_relay_addresses:");
    if effective.is_empty() {
        println!("  <none>");
    } else {
        for entry in effective {
            let source = match entry.source {
                fungi_config::RelayAddressSource::Community => "community",
                fungi_config::RelayAddressSource::Custom => "custom",
            };
            println!("  [{}] {}", source, entry.address);
        }
    }
}
