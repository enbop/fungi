pub mod fungi_control;
pub mod fungi_daemon;
pub mod fungi_init;
pub mod fungi_migrate;
pub mod fungi_relay;

use std::num::NonZeroU32;
use std::path::PathBuf;

use clap::{CommandFactory, Parser, Subcommand};
use fungi_config::{FungiDir, default_fungi_dir_name};

pub const DEFAULT_PING_COUNT: u32 = 4;

pub fn resolve_ping_count(count: Option<NonZeroU32>, watch: bool) -> u32 {
    if watch {
        0
    } else {
        count.map_or(DEFAULT_PING_COUNT, NonZeroU32::get)
    }
}

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
        help = "Path to the Fungi config directory, defaults to the channel-specific directory"
    )]
    pub fungi_dir: Option<String>,

    #[cfg(target_os = "android")]
    #[clap(
        long,
        default_value = "",
        help = "Set default device info string for this device, only used in Android"
    )]
    pub default_device_name: String,
}

impl FungiDir for CommonArgs {
    fn fungi_dir(&self) -> PathBuf {
        self.fungi_dir
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                home::home_dir()
                    .unwrap_or_else(|| {
                        panic!(
                            "Unable to determine home directory. Please provide --fungi-dir explicitly."
                        )
                    })
                    .join(default_fungi_dir_name())
            })
    }
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize a Fungi configuration, and generate a keypair
    Init(fungi_init::InitArgs),
    /// Migrate an existing Fungi configuration directory to the current schema
    Migrate(fungi_migrate::MigrateArgs),
    /// Start a Fungi daemon or daemon-managed background services
    Daemon(fungi_daemon::DaemonCommandArgs),

    /// Manage relay configuration for the local daemon
    #[command(subcommand)]
    Relay(fungi_control::RelayCommands),

    /// Show daemon information
    #[command(subcommand)]
    Info(fungi_control::InfoCommands),
    /// Manage runtime safety boundary settings
    #[command(subcommand, visible_alias = "sec")]
    Security(fungi_control::SecurityCommands),
    /// Manage services
    #[command(visible_alias = "svc")]
    Service(fungi_control::ServiceArgs),
    /// Query and administer remote peers
    #[command(subcommand, hide = true)]
    Peer(fungi_control::PeerCommands),
    /// Device discovery and saved devices
    Device(fungi_control::DeviceArgs),
    /// Connection observability and diagnostics
    #[command(subcommand, visible_alias = "conn")]
    Connection(fungi_control::ConnectionCommands),
    /// Ping all active connections to a device
    Ping {
        /// Device name to ping
        peer: fungi_control::PeerInput,
        /// Ping interval in milliseconds
        #[arg(long, default_value_t = 2000)]
        interval_ms: u32,
        /// Number of ping rounds to run (default: 4)
        #[arg(long, conflicts_with = "watch")]
        count: Option<NonZeroU32>,
        /// Continue pinging until interrupted
        #[arg(long, conflicts_with = "count", default_value_t = false)]
        watch: bool,
        /// Show detailed output
        #[arg(short, long, default_value_t = false)]
        verbose: bool,
    },
    #[cfg(feature = "wasi")]
    /// [WASI runtime] Run a WebAssembly module (re-exported wasmtime command)
    Run(wasmtime_cli::commands::RunCommand),
    /// Invoke a service by name
    #[command(external_subcommand)]
    Dynamic(Vec<String>),
}

pub fn exit_unknown_dynamic_subcommand(tokens: &[String]) -> ! {
    let mut argv = Vec::with_capacity(tokens.len() + 1);
    argv.push("fungi".to_string());
    argv.extend(tokens.iter().cloned());
    let mut command = FungiArgs::command();
    command.build();
    let mut command = command.allow_external_subcommands(false);
    match command.try_get_matches_from_mut(argv) {
        Ok(_) => unreachable!("dynamic subcommand unexpectedly matched without external support"),
        Err(error) => error.exit(),
    }
}
