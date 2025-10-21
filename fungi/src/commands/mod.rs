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

    #[cfg(target_os = "android")]
    #[clap(
        long,
        help = "Set default device info string for this device, only used in Android"
    )]
    pub default_device_name: String,
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
    /// Initialize a Fungi configuration, and generate a keypair
    Init(fungi_init::InitArgs),
    /// Start a Fungi daemon
    ///
    Daemon(fungi_daemon::DaemonArgs),
    /// Start a simple Fungi relay server
    Relay(fungi_relay::RelayArgs),

    /// Show daemon information
    #[command(subcommand)]
    Info(fungi_control::InfoCommands),
    /// Manage incoming allowed peers
    #[command(subcommand, visible_alias = "ap")]
    AllowedPeer(fungi_control::AllowedPeerCommands),
    /// Manage file transfer service
    #[command(subcommand, visible_alias = "fs")]
    FtService(fungi_control::FtServiceCommands),
    /// Manage file transfer client config and FTP and WebDAV proxies
    #[command(subcommand, visible_alias = "fc")]
    FtClient(fungi_control::FtClientCommands),
    /// Manage TCP tunneling
    #[command(subcommand, visible_alias = "tn")]
    Tunnel(fungi_control::TunnelCommands),
    /// Device discovery and address book
    #[command(subcommand)]
    Device(fungi_control::DeviceCommands),

    /// [WASI runtime] Run a WebAssembly module (re-exported wasmtime command)
    Run(wasmtime_cli::commands::RunCommand),
    /// [WASI runtime] Serve wasi-http requests (re-exported wasmtime command)
    Serve(wasmtime_cli::commands::ServeCommand),
}
