use clap::{Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, ValueEnum)]
pub enum TrustMode {
    Both,
    BTrustsA,
    ATrustsB,
    None,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum NodeName {
    A,
    B,
}

#[derive(Subcommand, Clone, Copy, Debug)]
pub enum ProcessCommand {
    Start,
    Stop,
    Restart,
}

#[derive(Subcommand, Clone, Copy, Debug)]
pub enum NodeCommand {
    Start { node: NodeName },
    Stop { node: NodeName },
    Restart { node: NodeName },
}

#[derive(Clone, Debug)]
pub(crate) struct ProcessSpec {
    pub(crate) label: &'static str,
    pub(crate) exe: Option<PathBuf>,
    pub(crate) cmd_contains: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct LabState {
    pub(crate) version: u32,
    pub(crate) repo: PathBuf,
    pub(crate) root: PathBuf,
    pub(crate) fungi_bin: PathBuf,
    pub(crate) manager_pid: Option<u32>,
    #[serde(default)]
    pub(crate) ready: bool,
    pub(crate) created_at_epoch_secs: u64,
    pub(crate) expires_at_epoch_secs: u64,
    pub(crate) trust: TrustMode,
    pub(crate) relay: RelayState,
    pub(crate) node_a: NodeState,
    pub(crate) node_b: NodeState,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct RelayState {
    pub(crate) pid: Option<u32>,
    pub(crate) home: PathBuf,
    pub(crate) log: PathBuf,
    pub(crate) peer_id: String,
    pub(crate) tcp_port: u16,
    pub(crate) udp_port: u16,
    pub(crate) tcp_addr: String,
    pub(crate) udp_addr: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct NodeState {
    pub(crate) name: String,
    pub(crate) pid: Option<u32>,
    pub(crate) dir: PathBuf,
    pub(crate) log: PathBuf,
    pub(crate) peer_id: String,
    pub(crate) rpc_port: u16,
    pub(crate) tcp_port: u16,
    pub(crate) udp_port: u16,
}

impl TrustMode {
    pub(crate) fn as_arg(self) -> &'static str {
        match self {
            TrustMode::Both => "both",
            TrustMode::BTrustsA => "b-trusts-a",
            TrustMode::ATrustsB => "a-trusts-b",
            TrustMode::None => "none",
        }
    }
}

impl NodeCommand {
    pub(crate) fn node(self) -> NodeName {
        match self {
            NodeCommand::Start { node }
            | NodeCommand::Stop { node }
            | NodeCommand::Restart { node } => node,
        }
    }
}

impl NodeState {
    pub(crate) fn empty(name: &str, dir: PathBuf) -> Self {
        Self {
            name: name.to_string(),
            pid: None,
            dir,
            log: PathBuf::new(),
            peer_id: String::new(),
            rpc_port: 0,
            tcp_port: 0,
            udp_port: 0,
        }
    }
}

impl LabState {
    pub(crate) fn node(&self, node: NodeName) -> &NodeState {
        match node {
            NodeName::A => &self.node_a,
            NodeName::B => &self.node_b,
        }
    }

    pub(crate) fn node_mut(&mut self, node: NodeName) -> &mut NodeState {
        match node {
            NodeName::A => &mut self.node_a,
            NodeName::B => &mut self.node_b,
        }
    }

    pub(crate) fn process_spec_for_manager(&self) -> ProcessSpec {
        ProcessSpec {
            label: "manager",
            exe: None,
            cmd_contains: vec![
                "fungi-lab".to_string(),
                "manager".to_string(),
                self.root.display().to_string(),
            ],
        }
    }

    pub(crate) fn process_spec_for_relay(&self) -> ProcessSpec {
        ProcessSpec {
            label: "relay",
            exe: Some(self.fungi_bin.clone()),
            cmd_contains: vec![
                "daemon".to_string(),
                "relay-server".to_string(),
                self.relay.tcp_port.to_string(),
                self.relay.udp_port.to_string(),
            ],
        }
    }

    pub(crate) fn process_spec_for_node(&self, node: NodeName) -> ProcessSpec {
        let node = self.node(node);
        ProcessSpec {
            label: if node.name == "a" { "node-a" } else { "node-b" },
            exe: Some(self.fungi_bin.clone()),
            cmd_contains: vec![
                "--fungi-dir".to_string(),
                node.dir.display().to_string(),
                "daemon".to_string(),
            ],
        }
    }
}
