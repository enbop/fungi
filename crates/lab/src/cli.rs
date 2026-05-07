use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

use crate::runtime::{
    apply_trust_mode_to_default_lab, clean_default_lab, manage_node, manage_relay, print_env,
    print_status, run_manager, start_background_lab, stop_default_lab,
};
use crate::state::{NodeCommand, ProcessCommand, TrustMode};

const DEFAULT_TTL_SECS: u64 = 2 * 60 * 60;

#[derive(Parser, Debug)]
#[command(
    version,
    about = "Create and manage a local Fungi relay + two-node lab"
)]
pub struct LabCli {
    #[command(subcommand)]
    command: LabCommand,
}

impl LabCli {
    pub fn run(self) -> Result<()> {
        match self.command {
            LabCommand::Start(args) => start_background_lab(args),
            LabCommand::Status(args) => print_status(args),
            LabCommand::Stop => stop_default_lab(),
            LabCommand::Clean => clean_default_lab(),
            LabCommand::Env => print_env(),
            LabCommand::Node { command } => manage_node(command),
            LabCommand::Relay { command } => manage_relay(command),
            LabCommand::Trust { mode } => apply_trust_mode_to_default_lab(mode),
            LabCommand::Manager(args) => run_manager(args),
        }
    }
}

#[derive(Subcommand, Debug)]
pub(crate) enum LabCommand {
    /// Start a background local relay + node-a + node-b lab.
    Start(StartArgs),
    /// Show the current local lab state.
    Status(StatusArgs),
    /// Stop lab processes but keep node directories and logs.
    Stop,
    /// Stop lab processes and delete target/tmp_a, target/tmp_b, and target/local-lab.
    Clean,
    /// Print shell exports for the current lab.
    Env,
    /// Stop, start, or restart one lab node.
    Node {
        #[command(subcommand)]
        command: NodeCommand,
    },
    /// Stop, start, or restart the local relay.
    Relay {
        #[command(subcommand)]
        command: ProcessCommand,
    },
    /// Reconfigure trusted-device direction between node-a and node-b.
    Trust {
        #[arg(value_enum)]
        mode: TrustMode,
    },
    #[command(hide = true)]
    Manager(ManagerArgs),
}

#[derive(Parser, Debug)]
pub struct StartArgs {
    /// Path to the fungi repo. Defaults to the nearest workspace root.
    #[arg(long)]
    pub(crate) repo: Option<PathBuf>,
    /// Path to the fungi binary. Defaults to target/debug/fungi next to this binary.
    #[arg(long = "fungi-bin")]
    pub(crate) fungi_bin: Option<PathBuf>,
    /// Lab state/log root. Defaults to target/local-lab.
    #[arg(long)]
    pub(crate) root: Option<PathBuf>,
    /// Node A fungi-dir. Defaults to target/tmp_a.
    #[arg(long = "node-a-dir")]
    pub(crate) node_a_dir: Option<PathBuf>,
    /// Node B fungi-dir. Defaults to target/tmp_b.
    #[arg(long = "node-b-dir")]
    pub(crate) node_b_dir: Option<PathBuf>,
    /// Seconds before the background manager stops all lab processes.
    #[arg(long, default_value_t = DEFAULT_TTL_SECS)]
    pub(crate) ttl_secs: u64,
    /// Trusted-device direction to configure after startup.
    #[arg(long, value_enum, default_value_t = TrustMode::BTrustsA)]
    pub(crate) trust: TrustMode,
    /// Refuse to replace an existing lab.
    #[arg(long)]
    pub(crate) no_replace: bool,
}

#[derive(Parser, Debug)]
pub(crate) struct StatusArgs {
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Parser, Debug, Clone)]
pub(crate) struct ManagerArgs {
    #[arg(long)]
    pub(crate) repo: PathBuf,
    #[arg(long = "fungi-bin")]
    pub(crate) fungi_bin: PathBuf,
    #[arg(long)]
    pub(crate) root: PathBuf,
    #[arg(long)]
    pub(crate) node_a_dir: PathBuf,
    #[arg(long)]
    pub(crate) node_b_dir: PathBuf,
    #[arg(long)]
    pub(crate) ttl_secs: u64,
    #[arg(long, value_enum)]
    pub(crate) trust: TrustMode,
}
