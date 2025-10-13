pub mod fungi_control;
pub mod fungi_daemon;
pub mod fungi_init;
pub mod fungi_relay;
pub mod fungi_wasi;

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use fungi_config::{DEFAULT_FUNGI_DIR, FungiDir};

/// A platform built for seamless multi-device integration
#[derive(Parser, Debug, Clone)]
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
