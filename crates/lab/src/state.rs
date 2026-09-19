use anyhow::{Context, Result, bail};
use clap::{Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use crate::process::ProcessId;

pub(crate) const STATE_FILE: &str = "state.json";
const VERSION: u32 = 3;
const MARKER: &str = ".fungi-lab";
pub(crate) const LOCK_FILE: &str = ".fungi-lab.lock";
const OWNER: &str = "fungi-lab\n";

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum TrustMode {
    None,
    BTrustsA,
    ATrustsB,
    Both,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Target {
    Relay,
    A,
    B,
}

impl Target {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Relay => "relay",
            Self::A => "node-a",
            Self::B => "node-b",
        }
    }
}
impl From<NodeName> for Target {
    fn from(node: NodeName) -> Self {
        match node {
            NodeName::A => Self::A,
            NodeName::B => Self::B,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(crate) struct NodeState {
    pub(crate) process: Option<ProcessId>,
    pub(crate) peer_id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct LabState {
    version: u32,
    pub(crate) fungi_bin: PathBuf,
    pub(crate) relay: NodeState,
    pub(crate) relay_tcp_port: u16,
    pub(crate) relay_udp_port: u16,
    pub(crate) node_a: NodeState,
    pub(crate) node_b: NodeState,
}

pub(crate) struct Lab {
    pub(crate) root: PathBuf,
    pub(crate) state: LabState,
}

impl Lab {
    pub(crate) fn new(root: &Path, fungi_bin: PathBuf, tcp: u16, udp: u16) -> Self {
        Self {
            root: root.to_path_buf(),
            state: LabState {
                version: VERSION,
                fungi_bin,
                relay: NodeState::default(),
                relay_tcp_port: tcp,
                relay_udp_port: udp,
                node_a: NodeState::default(),
                node_b: NodeState::default(),
            },
        }
    }

    pub(crate) fn load(root: &Path) -> Result<Self> {
        validate_layout(root)?;
        let raw = fs::read(root.join(STATE_FILE))
            .context("cannot read lab state; use start to create a lab")?;
        let value: serde_json::Value =
            serde_json::from_slice(&raw).context("invalid lab state; retained for inspection")?;
        if value["version"].as_u64() != Some(VERSION as u64) {
            bail!(
                "unsupported lab state version {}; expected {VERSION}. Stop/clean with the previous binary, or choose a fresh --lab-dir",
                value["version"]
            );
        }
        let state: LabState = serde_json::from_value(value)?;
        if !state.fungi_bin.is_absolute() {
            bail!("lab binary path must be absolute");
        }
        Ok(Self {
            root: root.to_path_buf(),
            state,
        })
    }

    pub(crate) fn save(&self) -> Result<()> {
        let mut file = tempfile::NamedTempFile::new_in(&self.root)?;
        serde_json::to_writer_pretty(&mut file, &self.state)?;
        file.as_file().sync_all()?;
        file.persist(self.root.join(STATE_FILE))
            .context("failed to publish lab state")?;
        Ok(())
    }

    pub(crate) fn node(&self, target: Target) -> &NodeState {
        match target {
            Target::Relay => &self.state.relay,
            Target::A => &self.state.node_a,
            Target::B => &self.state.node_b,
        }
    }
    pub(crate) fn node_mut(&mut self, target: Target) -> &mut NodeState {
        match target {
            Target::Relay => &mut self.state.relay,
            Target::A => &mut self.state.node_a,
            Target::B => &mut self.state.node_b,
        }
    }
    pub(crate) fn dir(&self, target: Target) -> PathBuf {
        self.root.join(match target {
            Target::Relay => "relay-home",
            Target::A => "nodes/a/fungi",
            Target::B => "nodes/b/fungi",
        })
    }
    pub(crate) fn log(&self, target: Target) -> PathBuf {
        self.root.join(format!("{}.log", target.label()))
    }
    pub(crate) fn relay_addresses(&self) -> [String; 2] {
        [
            format!(
                "/ip4/127.0.0.1/tcp/{}/p2p/{}",
                self.state.relay_tcp_port, self.state.relay.peer_id
            ),
            format!(
                "/ip4/127.0.0.1/udp/{}/quic-v1/p2p/{}",
                self.state.relay_udp_port, self.state.relay.peer_id
            ),
        ]
    }
}

// One lock for each mutating invocation. There is no background state writer.
pub(crate) fn lock_lab(root: &Path, initialize: bool) -> Result<File> {
    reject_symlink(root)?;
    if initialize {
        fs::create_dir_all(root)?;
        if !root.join(MARKER).exists() {
            if fs::read_dir(root)?.next().is_some() {
                bail!("lab directory is not empty; choose a fresh --lab-dir");
            }
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(root.join(MARKER))?;
            file.write_all(OWNER.as_bytes())?;
            file.sync_all()?;
        }
    }
    validate_layout(root)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join(LOCK_FILE))?;
    // Windows locks also prevent reads through other handles. Keep the
    // ownership marker readable while a mutating command holds the lock.
    file.try_lock()
        .context("another command is managing this lab; try again after it finishes")?;
    Ok(file)
}

fn reject_symlink(path: &Path) -> Result<()> {
    if fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        bail!("refusing symlink in lab layout: {}", path.display());
    }
    Ok(())
}

pub(crate) fn validate_layout(root: &Path) -> Result<()> {
    reject_symlink(root)?;
    for name in [
        MARKER,
        LOCK_FILE,
        STATE_FILE,
        "relay-home",
        "relay.log",
        "node-a.log",
        "node-b.log",
        "nodes",
    ] {
        reject_symlink(&root.join(name))?;
    }
    for node in ["a", "b"] {
        let parent = root.join("nodes").join(node);
        reject_symlink(&parent)?;
        for name in ["fungi", "Fungi", "FungiDev", "fungi/config.toml"] {
            reject_symlink(&parent.join(name))?;
        }
    }
    if fs::read_to_string(root.join(MARKER)).context("unrecognized lab directory; use the previous tool for old labs, or choose a fresh --lab-dir")? != OWNER {
        bail!("unrecognized lab directory version; use the previous tool to stop/clean it");
    }
    Ok(())
}

pub(crate) fn selected_root(root: Option<PathBuf>) -> Result<PathBuf> {
    let root = match root {
        Some(root) => root,
        None => find_repo_root()?.join("target/local-lab"),
    };
    let absolute = std::path::absolute(root)?;
    if absolute
        .components()
        .any(|c| c == std::path::Component::ParentDir)
    {
        bail!("lab directory must not contain '..'");
    }
    reject_symlink(&absolute)?;
    // Canonicalize after initialization too, so aliases of a parent share paths.
    if absolute.exists() {
        return Ok(absolute.canonicalize()?);
    }
    Ok(absolute)
}

pub(crate) fn find_repo_root() -> Result<PathBuf> {
    for start in [std::env::current_dir()?, std::env::current_exe()?] {
        for path in start.ancestors() {
            if path.join("Cargo.toml").is_file() && path.join("fungi/Cargo.toml").is_file() {
                return Ok(path.to_path_buf());
            }
        }
    }
    bail!("cannot find Fungi checkout; supply --lab-dir")
}
