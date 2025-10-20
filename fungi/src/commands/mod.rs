pub mod fungi_control;
pub mod fungi_daemon;
pub mod fungi_init;
pub mod fungi_relay;

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use fungi_config::{DEFAULT_FUNGI_DIR, FungiDir};

/// A platform built for seamless multi-device integration
#[derive(Parser)]
#[command(version, about, long_about = None)]
pub struct FungiArgs {
    #[command(flatten)]
    pub common: CommonArgs,
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Clone, Default, Parser)]
pub struct CommonArgs {
    #[clap(
        short,
        long,
        help = "Path to the Fungi config directory, defaults to ~/.fungi"
    )]
    pub fungi_dir: Option<String>,
}

impl FungiDir for CommonArgs {
    fn fungi_dir(&self) -> PathBuf {
        self.fungi_dir
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| home::home_dir().unwrap().join(DEFAULT_FUNGI_DIR))
    }
}

#[derive(Subcommand)]
pub enum Commands {
    /// Start a Fungi daemon
    Daemon(fungi_daemon::DaemonArgs),
    /// Initialize a Fungi configuration, and generate a keypair
    Init(fungi_init::InitArgs),
    /// Start a simple Fungi relay server
    Relay(fungi_relay::RelayArgs),
    /// Runs a WebAssembly module (re-exported wasmtime command)
    Run(wasmtime_cli::commands::RunCommand),
    /// Serves requests from a wasi-http proxy component (re-exported wasmtime command)
    Serve(wasmtime_cli::commands::ServeCommand),

    /// Daemon information commands
    #[command(subcommand)]
    Info(fungi_control::InfoCommands),
    /// Manage incoming connection allowlist
    #[command(subcommand)]
    Peer(fungi_control::PeerCommands),
    /// File transfer service management
    #[command(subcommand)]
    Ft(fungi_control::FileTransferCommands),
    /// FTP and WebDAV proxy management
    #[command(subcommand)]
    Proxy(fungi_control::ProxyCommands),
    /// TCP tunneling management
    #[command(subcommand)]
    Tunnel(fungi_control::TunnelCommands),
    /// Device discovery and address book
    #[command(subcommand)]
    Device(fungi_control::DeviceCommands),
}
