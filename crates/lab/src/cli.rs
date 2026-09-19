use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

use crate::runtime;
use crate::state::{Lab, NodeCommand, ProcessCommand, Target, TrustMode, lock_lab, selected_root};

#[derive(Parser, Debug)]
#[command(
    version,
    about = "Create and manage a local Fungi relay + two-node lab"
)]
pub struct LabCli {
    /// Lab data directory (not a source checkout or a node's fungi-dir).
    #[arg(
        long = "lab-dir",
        value_name = "PATH",
        global = true,
        env = "FUNGI_LAB_DIR"
    )]
    root: Option<PathBuf>,
    #[command(subcommand)]
    command: LabCommand,
}

impl LabCli {
    pub fn run(self) -> Result<()> {
        let root = selected_root(self.root)?;
        let _lock = match &self.command {
            LabCommand::Status(_) | LabCommand::Env => None,
            LabCommand::Start(_) => Some(lock_lab(&root, true)?),
            _ => Some(lock_lab(&root, false)?),
        };
        let root = root.canonicalize()?;
        match self.command {
            LabCommand::Start(args) => runtime::start(&root, args),
            LabCommand::Status(args) => runtime::print_status(&Lab::load(&root)?, args.json),
            LabCommand::Stop => Lab::load(&root)?.stop(&[Target::A, Target::B, Target::Relay]),
            LabCommand::Clean => runtime::clean(&root),
            LabCommand::Env => runtime::print_env(&Lab::load(&root)?),
            LabCommand::Node { command } => {
                let (node, operation) = match command {
                    NodeCommand::Start { node } => (node, ProcessCommand::Start),
                    NodeCommand::Stop { node } => (node, ProcessCommand::Stop),
                    NodeCommand::Restart { node } => (node, ProcessCommand::Restart),
                };
                Lab::load(&root)?.manage(node.into(), operation)
            }
            LabCommand::Relay { command } => Lab::load(&root)?.manage(Target::Relay, command),
            LabCommand::Trust { mode } => {
                Lab::load(&root)?.trust(mode, std::time::Instant::now() + runtime::STARTUP_TIMEOUT)
            }
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
    /// Stop lab processes and remove the owned lab data directory.
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
}

#[derive(Parser, Debug)]
pub(crate) struct StartArgs {
    /// Path to the fungi binary. Defaults to target/debug/fungi next to this binary.
    #[arg(long = "fungi-bin")]
    pub(crate) fungi_bin: Option<PathBuf>,
    /// Trusted-device direction to configure after startup.
    #[arg(long, value_enum, default_value_t = TrustMode::None)]
    pub(crate) trust: TrustMode,
}

#[derive(Parser, Debug)]
pub(crate) struct StatusArgs {
    #[arg(long)]
    pub(crate) json: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_defaults_to_no_trust() {
        let cli = LabCli::try_parse_from(["fungi-lab", "start"]).unwrap();
        let LabCommand::Start(args) = cli.command else {
            panic!("expected start command");
        };
        assert_eq!(args.trust, TrustMode::None);
    }

    #[test]
    fn root_is_a_global_selector() {
        for args in [
            vec!["fungi-lab", "--lab-dir", "/tmp/lab", "status"],
            vec!["fungi-lab", "status", "--lab-dir", "/tmp/lab"],
        ] {
            let cli = LabCli::try_parse_from(args).unwrap();
            assert_eq!(cli.root, Some(PathBuf::from("/tmp/lab")));
        }
    }
}
