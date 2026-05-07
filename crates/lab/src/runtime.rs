use anyhow::{Context, Result, bail};
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::Duration,
};

use crate::cli::{ManagerArgs, StartArgs, StatusArgs};
use crate::process::{
    detach_process_group, get_fungi_binary_path, process_is_running, reserve_tcp_port,
    reserve_udp_port, stop_pid,
};
use crate::state::{
    LabState, NodeCommand, NodeName, NodeState, ProcessCommand, RelayState, TrustMode,
};
use crate::util::{
    circuit_addr, default_root, display_path_arg, epoch_secs, find_repo_root, print_process,
    print_started_summary, read_state, run_cli_capture, run_cli_status, shell_quote,
    shell_quote_path, wait_for_state, wait_peer_id, wait_relay_peer_id_from_log, write_node_config,
    write_state,
};

const STATE_FILE: &str = crate::util::STATE_FILE;
const MANAGER_LOG: &str = "manager.log";
const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
const STATE_VERSION: u32 = 1;

#[derive(Clone, Debug)]
pub struct LabOptions {
    pub repo: PathBuf,
    pub fungi_bin: PathBuf,
    pub root: PathBuf,
    pub node_a_dir: PathBuf,
    pub node_b_dir: PathBuf,
    pub ttl_secs: u64,
    pub trust: TrustMode,
    pub replace: bool,
}

#[derive(Clone, Debug)]
pub struct LocalLab {
    state: LabState,
}

impl LocalLab {
    pub fn start(options: LabOptions) -> Result<Self> {
        let root = options.root.clone();
        start_background_lab(StartArgs {
            repo: Some(options.repo),
            fungi_bin: Some(options.fungi_bin),
            root: Some(options.root),
            node_a_dir: Some(options.node_a_dir),
            node_b_dir: Some(options.node_b_dir),
            ttl_secs: options.ttl_secs,
            trust: options.trust,
            no_replace: !options.replace,
        })?;
        let state = read_state(&root)?;
        Ok(Self { state })
    }

    pub fn status(root: &Path) -> Result<Self> {
        Ok(Self {
            state: read_state(root)?,
        })
    }

    pub fn stop(&self) -> Result<()> {
        stop_lab(&self.state)
    }

    pub fn node_dir(&self, node: NodeName) -> &Path {
        match node {
            NodeName::A => &self.state.node_a.dir,
            NodeName::B => &self.state.node_b.dir,
        }
    }

    pub fn node_peer_id(&self, node: NodeName) -> &str {
        match node {
            NodeName::A => &self.state.node_a.peer_id,
            NodeName::B => &self.state.node_b.peer_id,
        }
    }

    pub fn run_cli<I, S>(&self, node: NodeName, args: I) -> Result<String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        run_cli_capture(
            &self.state.fungi_bin,
            &self.state.repo,
            self.node_dir(node),
            args,
            None,
        )
    }
}

pub(crate) fn start_background_lab(args: StartArgs) -> Result<()> {
    let repo = match args.repo {
        Some(repo) => repo,
        None => find_repo_root()?,
    };
    let repo = repo.canonicalize().unwrap_or(repo);
    let fungi_bin = match args.fungi_bin {
        Some(path) => path,
        None => get_fungi_binary_path()?,
    };
    let fungi_bin = fungi_bin.canonicalize().unwrap_or(fungi_bin);
    let root = args.root.unwrap_or_else(|| repo.join("target/local-lab"));
    let node_a_dir = args.node_a_dir.unwrap_or_else(|| repo.join("target/tmp_a"));
    let node_b_dir = args.node_b_dir.unwrap_or_else(|| repo.join("target/tmp_b"));

    if root.join(STATE_FILE).exists() {
        let state = read_state(&root)?;
        if args.no_replace
            && process_is_running(state.manager_pid, &state.process_spec_for_manager())
        {
            bail!("local lab is already running. Use `fungi-lab stop` first.");
        }
        stop_lab(&state).context("failed to stop previous local lab; refusing to start over it")?;
    }

    fs::create_dir_all(&root)?;
    let manager_log = root.join(MANAGER_LOG);
    let stdout = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&manager_log)
        .with_context(|| format!("failed to open {}", manager_log.display()))?;
    let stderr = stdout
        .try_clone()
        .with_context(|| format!("failed to clone {}", manager_log.display()))?;

    let current_exe = std::env::current_exe().context("failed to locate fungi-lab binary")?;
    let mut command = Command::new(current_exe);
    command
        .arg("manager")
        .arg("--repo")
        .arg(&repo)
        .arg("--fungi-bin")
        .arg(&fungi_bin)
        .arg("--root")
        .arg(&root)
        .arg("--node-a-dir")
        .arg(&node_a_dir)
        .arg("--node-b-dir")
        .arg(&node_b_dir)
        .arg("--ttl-secs")
        .arg(args.ttl_secs.to_string())
        .arg("--trust")
        .arg(args.trust.as_arg())
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    detach_process_group(&mut command);
    let mut child = command
        .spawn()
        .context("failed to start fungi-lab manager")?;

    let state = wait_for_state(&root, STARTUP_TIMEOUT).inspect_err(|_| {
        let _ = child.kill();
        let _ = child.wait();
    })?;
    print_started_summary(&state);
    Ok(())
}

pub(crate) fn run_manager(args: ManagerArgs) -> Result<()> {
    fs::create_dir_all(&args.root)?;
    let now = epoch_secs();
    let mut state = LabState {
        version: STATE_VERSION,
        repo: args.repo.clone(),
        root: args.root.clone(),
        fungi_bin: args.fungi_bin.clone(),
        manager_pid: Some(std::process::id()),
        ready: false,
        created_at_epoch_secs: now,
        expires_at_epoch_secs: now.saturating_add(args.ttl_secs),
        trust: args.trust,
        relay: start_relay(&args)?,
        node_a: NodeState::empty("a", args.node_a_dir.clone()),
        node_b: NodeState::empty("b", args.node_b_dir.clone()),
    };
    write_state(&state)?;

    state.node_a = start_node(&state, NodeName::A)?;
    write_state(&state)?;
    state.node_b = start_node(&state, NodeName::B)?;
    write_state(&state)?;

    add_lab_devices(&state)?;
    apply_trust_mode(&state, state.trust)?;
    state.ready = true;
    write_state(&state)?;

    loop {
        thread::sleep(Duration::from_secs(1));
        if epoch_secs() >= state.expires_at_epoch_secs {
            let latest = read_state(&state.root).unwrap_or(state.clone());
            let _ = stop_lab_processes(&latest, true);
            return Ok(());
        }
    }
}

pub(crate) fn print_status(args: StatusArgs) -> Result<()> {
    let root = default_root()?;
    let state = read_state(&root)?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&state)?);
        return Ok(());
    }

    println!("Fungi local lab");
    println!("  root: {}", state.root.display());
    println!("  ready: {}", state.ready);
    println!("  expires_at_epoch_secs: {}", state.expires_at_epoch_secs);
    print_process(
        "manager",
        state.manager_pid,
        None,
        None,
        &state.process_spec_for_manager(),
    );
    print_process(
        "relay",
        state.relay.pid,
        Some(&state.relay.peer_id),
        Some(&state.relay.log),
        &state.process_spec_for_relay(),
    );
    print_process(
        "node-a",
        state.node_a.pid,
        Some(&state.node_a.peer_id),
        Some(&state.node_a.log),
        &state.process_spec_for_node(NodeName::A),
    );
    print_process(
        "node-b",
        state.node_b.pid,
        Some(&state.node_b.peer_id),
        Some(&state.node_b.log),
        &state.process_spec_for_node(NodeName::B),
    );
    println!(
        "  fungi a: {} -f {}",
        state.fungi_bin.display(),
        display_path_arg(&state.repo, &state.node_a.dir)
    );
    println!(
        "  fungi b: {} -f {}",
        state.fungi_bin.display(),
        display_path_arg(&state.repo, &state.node_b.dir)
    );
    Ok(())
}

pub(crate) fn stop_default_lab() -> Result<()> {
    let state = read_state(&default_root()?)?;
    stop_lab(&state)?;
    let mut state = state;
    state.ready = false;
    state.manager_pid = None;
    state.relay.pid = None;
    state.node_a.pid = None;
    state.node_b.pid = None;
    let _ = write_state(&state);
    println!("Stopped Fungi local lab processes.");
    Ok(())
}

pub(crate) fn clean_default_lab() -> Result<()> {
    let root = default_root()?;
    if let Ok(state) = read_state(&root) {
        let _ = stop_lab(&state);
        let _ = fs::remove_dir_all(&state.node_a.dir);
        let _ = fs::remove_dir_all(&state.node_b.dir);
    }
    let _ = fs::remove_dir_all(&root);
    println!("Removed Fungi local lab directories.");
    Ok(())
}

pub(crate) fn print_env() -> Result<()> {
    let state = read_state(&default_root()?)?;
    println!("export FUNGI_BIN={}", shell_quote_path(&state.fungi_bin));
    println!("export FUNGI_LAB_ROOT={}", shell_quote_path(&state.root));
    println!("export FUNGI_A_DIR={}", shell_quote_path(&state.node_a.dir));
    println!("export FUNGI_B_DIR={}", shell_quote_path(&state.node_b.dir));
    println!(
        "export FUNGI_A_PEER_ID={}",
        shell_quote(&state.node_a.peer_id)
    );
    println!(
        "export FUNGI_B_PEER_ID={}",
        shell_quote(&state.node_b.peer_id)
    );
    println!(
        "export FUNGI_RELAY_TCP_ADDR={}",
        shell_quote(&state.relay.tcp_addr)
    );
    println!(
        "export FUNGI_RELAY_UDP_ADDR={}",
        shell_quote(&state.relay.udp_addr)
    );
    Ok(())
}

pub(crate) fn manage_node(command: NodeCommand) -> Result<()> {
    let root = default_root()?;
    let mut state = read_state(&root)?;
    match command {
        NodeCommand::Stop { node } => {
            let spec = state.process_spec_for_node(node);
            let node_state = state.node_mut(node);
            stop_pid(node_state.pid, &spec, true)?;
            node_state.pid = None;
        }
        NodeCommand::Start { node } => {
            let current_pid = state.node(node).pid;
            if process_is_running(current_pid, &state.process_spec_for_node(node)) {
                println!("node {:?} is already running.", node);
                return Ok(());
            }
            let updated = start_node(&state, node)?;
            *state.node_mut(node) = updated;
        }
        NodeCommand::Restart { node } => {
            {
                let spec = state.process_spec_for_node(node);
                let node_state = state.node_mut(node);
                stop_pid(node_state.pid, &spec, true)?;
                node_state.pid = None;
            }
            let updated = start_node(&state, node)?;
            *state.node_mut(node) = updated;
        }
    }
    write_state(&state)?;
    println!("Updated node {:?}.", command.node());
    Ok(())
}

pub(crate) fn manage_relay(command: ProcessCommand) -> Result<()> {
    let root = default_root()?;
    let mut state = read_state(&root)?;
    match command {
        ProcessCommand::Stop => {
            stop_pid(state.relay.pid, &state.process_spec_for_relay(), true)?;
            state.relay.pid = None;
        }
        ProcessCommand::Start => {
            if process_is_running(state.relay.pid, &state.process_spec_for_relay()) {
                println!("relay is already running.");
                return Ok(());
            }
            state.relay = restart_relay_from_state(&state)?;
        }
        ProcessCommand::Restart => {
            stop_pid(state.relay.pid, &state.process_spec_for_relay(), true)?;
            state.relay.pid = None;
            state.relay = restart_relay_from_state(&state)?;
        }
    }
    write_state(&state)?;
    println!("Updated relay.");
    Ok(())
}

pub(crate) fn apply_trust_mode_to_default_lab(mode: TrustMode) -> Result<()> {
    let root = default_root()?;
    let mut state = read_state(&root)?;
    apply_trust_mode(&state, mode)?;
    state.trust = mode;
    write_state(&state)?;
    println!("Trust mode set to {:?}.", mode);
    Ok(())
}

fn start_relay(args: &ManagerArgs) -> Result<RelayState> {
    let relay_home = args.root.join("relay-home");
    let relay_log = args.root.join("relay.log");
    fs::create_dir_all(&relay_home)?;
    let tcp_port = reserve_tcp_port()?;
    let udp_port = reserve_udp_port()?;
    let stdout = File::create(&relay_log)
        .with_context(|| format!("failed to create {}", relay_log.display()))?;
    let stderr = stdout
        .try_clone()
        .with_context(|| format!("failed to clone {}", relay_log.display()))?;
    let mut command = Command::new(&args.fungi_bin);
    command
        .env("HOME", &relay_home)
        .arg("daemon")
        .arg("relay-server")
        .arg("--public-ip")
        .arg("127.0.0.1")
        .arg("--tcp-listen-port")
        .arg(tcp_port.to_string())
        .arg("--udp-listen-port")
        .arg(udp_port.to_string())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    detach_process_group(&mut command);
    let child = command.spawn().context("failed to start local relay")?;
    let peer_id = wait_relay_peer_id_from_log(&relay_log, STARTUP_TIMEOUT)?;
    let tcp_addr = format!("/ip4/127.0.0.1/tcp/{tcp_port}/p2p/{peer_id}");
    let udp_addr = format!("/ip4/127.0.0.1/udp/{udp_port}/quic-v1/p2p/{peer_id}");

    Ok(RelayState {
        pid: Some(child.id()),
        home: relay_home,
        log: relay_log,
        peer_id,
        tcp_port,
        udp_port,
        tcp_addr,
        udp_addr,
    })
}

fn restart_relay_from_state(state: &LabState) -> Result<RelayState> {
    let args = ManagerArgs {
        repo: state.repo.clone(),
        fungi_bin: state.fungi_bin.clone(),
        root: state.root.clone(),
        node_a_dir: state.node_a.dir.clone(),
        node_b_dir: state.node_b.dir.clone(),
        ttl_secs: state
            .expires_at_epoch_secs
            .saturating_sub(state.created_at_epoch_secs),
        trust: state.trust,
    };
    let relay = start_relay(&args)?;
    if relay.peer_id != state.relay.peer_id {
        bail!(
            "relay peer id changed from {} to {}; run `fungi-lab clean` before reusing node configs",
            state.relay.peer_id,
            relay.peer_id
        );
    }
    Ok(relay)
}

fn start_node(state: &LabState, node: NodeName) -> Result<NodeState> {
    let (name, dir) = match node {
        NodeName::A => ("a", state.node_a.dir.clone()),
        NodeName::B => ("b", state.node_b.dir.clone()),
    };
    fs::create_dir_all(&dir)?;
    run_cli_status(&state.fungi_bin, &state.repo, &dir, ["init"], None)?;

    let rpc_port = reserve_tcp_port()?;
    let tcp_port = reserve_tcp_port()?;
    let udp_port = reserve_udp_port()?;
    write_node_config(
        &dir,
        rpc_port,
        tcp_port,
        udp_port,
        &[state.relay.tcp_addr.clone(), state.relay.udp_addr.clone()],
    )?;

    let log = state.root.join(format!("node-{name}.log"));
    let stdout =
        File::create(&log).with_context(|| format!("failed to create {}", log.display()))?;
    let stderr = stdout
        .try_clone()
        .with_context(|| format!("failed to clone {}", log.display()))?;
    let mut command = Command::new(&state.fungi_bin);
    command
        .arg("--fungi-dir")
        .arg(&dir)
        .arg("daemon")
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    detach_process_group(&mut command);
    let child = command
        .spawn()
        .with_context(|| format!("failed to start node-{name}"))?;
    let peer_id = wait_peer_id(&state.fungi_bin, &state.repo, &dir, STARTUP_TIMEOUT)?;

    Ok(NodeState {
        name: name.to_string(),
        pid: Some(child.id()),
        dir,
        log,
        peer_id,
        rpc_port,
        tcp_port,
        udp_port,
    })
}

fn add_lab_devices(state: &LabState) -> Result<()> {
    let a_relay = circuit_addr(&state.relay.tcp_addr, &state.node_a.peer_id);
    let b_relay = circuit_addr(&state.relay.tcp_addr, &state.node_b.peer_id);
    run_cli_status(
        &state.fungi_bin,
        &state.repo,
        &state.node_a.dir,
        [
            "device",
            "add",
            "b",
            &state.node_b.peer_id,
            "--addr",
            &b_relay,
        ],
        None,
    )?;
    run_cli_status(
        &state.fungi_bin,
        &state.repo,
        &state.node_b.dir,
        [
            "device",
            "add",
            "a",
            &state.node_a.peer_id,
            "--addr",
            &a_relay,
        ],
        None,
    )?;
    Ok(())
}

fn apply_trust_mode(state: &LabState, mode: TrustMode) -> Result<()> {
    set_trust(
        state,
        NodeName::A,
        &state.node_b.peer_id,
        matches!(mode, TrustMode::Both | TrustMode::ATrustsB),
    )?;
    set_trust(
        state,
        NodeName::B,
        &state.node_a.peer_id,
        matches!(mode, TrustMode::Both | TrustMode::BTrustsA),
    )?;
    Ok(())
}

fn set_trust(state: &LabState, node: NodeName, peer_id: &str, trusted: bool) -> Result<()> {
    let dir = state.node(node).dir.clone();
    let command = if trusted { "trust" } else { "untrust" };
    run_cli_status(
        &state.fungi_bin,
        &state.repo,
        &dir,
        ["device", command, peer_id],
        if trusted { Some("y\n") } else { None },
    )?;
    Ok(())
}

pub(crate) fn stop_lab(state: &LabState) -> Result<()> {
    stop_lab_processes(state, false)
}

fn stop_lab_processes(state: &LabState, from_manager: bool) -> Result<()> {
    stop_pid(
        state.node_a.pid,
        &state.process_spec_for_node(NodeName::A),
        true,
    )?;
    stop_pid(
        state.node_b.pid,
        &state.process_spec_for_node(NodeName::B),
        true,
    )?;
    stop_pid(state.relay.pid, &state.process_spec_for_relay(), true)?;
    if !from_manager {
        stop_pid(state.manager_pid, &state.process_spec_for_manager(), true)?;
    }
    Ok(())
}
