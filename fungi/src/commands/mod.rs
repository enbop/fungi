pub mod fungi_control;
pub mod fungi_daemon;
pub mod fungi_init;
pub mod fungi_relay;
pub mod fungi_wasi;

use clap::{Parser, Subcommand};

/// A platform built for seamless multi-device integration
#[derive(Parser, Debug, Clone)]
#[command(version, about, long_about = None)]
pub struct FungiArgs {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Start a Fungi daemon
    Daemon(fungi_daemon::DaemonArgs),
    /// Initialize a Fungi configuration, and generate a keypair
    Init(fungi_init::InitArgs),
    /// Start a simple Fungi relay server
    Relay(fungi_relay::RelayArgs),
    /// Run a WASI module
    Run(fungi_wasi::WasiArgs),

    #[command(flatten)]
    Control(fungi_control::ControlCommands),
}
