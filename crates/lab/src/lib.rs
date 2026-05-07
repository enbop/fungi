use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader},
    net::{TcpListener, UdpSocket},
    path::{Path, PathBuf},
    process::{Child, ChildStdout, Command, Stdio},
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use sysinfo::{Pid, Process, Signal, System};

const STATE_FILE: &str = "state.json";
const MANAGER_LOG: &str = "manager.log";
const DEFAULT_TTL_SECS: u64 = 2 * 60 * 60;
const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
const STATE_VERSION: u32 = 1;

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
enum LabCommand {
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
    repo: Option<PathBuf>,
    /// Path to the fungi binary. Defaults to target/debug/fungi next to this binary.
    #[arg(long = "fungi-bin")]
    fungi_bin: Option<PathBuf>,
    /// Lab state/log root. Defaults to target/local-lab.
    #[arg(long)]
    root: Option<PathBuf>,
    /// Node A fungi-dir. Defaults to target/tmp_a.
    #[arg(long = "node-a-dir")]
    node_a_dir: Option<PathBuf>,
    /// Node B fungi-dir. Defaults to target/tmp_b.
    #[arg(long = "node-b-dir")]
    node_b_dir: Option<PathBuf>,
    /// Seconds before the background manager stops all lab processes.
    #[arg(long, default_value_t = DEFAULT_TTL_SECS)]
    ttl_secs: u64,
    /// Trusted-device direction to configure after startup.
    #[arg(long, value_enum, default_value_t = TrustMode::BTrustsA)]
    trust: TrustMode,
    /// Refuse to replace an existing lab.
    #[arg(long)]
    no_replace: bool,
}

#[derive(Parser, Debug)]
struct StatusArgs {
    #[arg(long)]
    json: bool,
}

#[derive(Parser, Debug, Clone)]
struct ManagerArgs {
    #[arg(long)]
    repo: PathBuf,
    #[arg(long = "fungi-bin")]
    fungi_bin: PathBuf,
    #[arg(long)]
    root: PathBuf,
    #[arg(long)]
    node_a_dir: PathBuf,
    #[arg(long)]
    node_b_dir: PathBuf,
    #[arg(long)]
    ttl_secs: u64,
    #[arg(long, value_enum)]
    trust: TrustMode,
}

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
struct ProcessSpec {
    label: &'static str,
    exe: Option<PathBuf>,
    cmd_contains: Vec<String>,
}

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

#[derive(Clone, Debug, Deserialize, Serialize)]
struct LabState {
    version: u32,
    repo: PathBuf,
    root: PathBuf,
    fungi_bin: PathBuf,
    manager_pid: Option<u32>,
    #[serde(default)]
    ready: bool,
    created_at_epoch_secs: u64,
    expires_at_epoch_secs: u64,
    trust: TrustMode,
    relay: RelayState,
    node_a: NodeState,
    node_b: NodeState,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct RelayState {
    pid: Option<u32>,
    home: PathBuf,
    log: PathBuf,
    peer_id: String,
    tcp_port: u16,
    udp_port: u16,
    tcp_addr: String,
    udp_addr: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct NodeState {
    name: String,
    pid: Option<u32>,
    dir: PathBuf,
    log: PathBuf,
    peer_id: String,
    rpc_port: u16,
    tcp_port: u16,
    udp_port: u16,
}

pub fn get_fungi_binary_path() -> Result<PathBuf> {
    let current_exe = std::env::current_exe().context("failed to get current executable")?;
    let target_dir = current_exe
        .parent()
        .context("failed to get current executable directory")?;
    let fungi_bin = target_dir.join("fungi");

    if !fungi_bin.exists() {
        bail!(
            "fungi binary not found at {}. Build it first with `cargo build --bin fungi`.",
            fungi_bin.display()
        );
    }

    Ok(fungi_bin)
}

pub fn init_fungi_dir(fungi_dir: &Path) -> Result<()> {
    let fungi_bin = get_fungi_binary_path()?;
    let init_status = Command::new(&fungi_bin)
        .arg("--fungi-dir")
        .arg(fungi_dir)
        .arg("init")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("failed to initialize fungi dir")?;
    if !init_status.success() {
        bail!("failed to initialize fungi dir: {init_status}");
    }
    Ok(())
}

pub fn patch_rpc_port(config_file: &Path, port: u16) -> Result<()> {
    let content = fs::read_to_string(config_file)
        .with_context(|| format!("failed to read {}", config_file.display()))?;
    let updated = content.replace(
        "listen_address = \"127.0.0.1:5405\"",
        &format!("listen_address = \"127.0.0.1:{port}\""),
    );
    fs::write(config_file, updated)
        .with_context(|| format!("failed to write {}", config_file.display()))?;
    Ok(())
}

pub fn assert_contains(haystack: &str, needle: &str) -> Result<()> {
    if haystack.contains(needle) {
        return Ok(());
    }
    bail!("expected to find `{needle}` in:\n{haystack}")
}

pub fn reserve_tcp_port() -> Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).context("failed to bind tcp probe")?;
    Ok(listener.local_addr()?.port())
}

pub fn reserve_udp_port() -> Result<u16> {
    let socket = UdpSocket::bind(("127.0.0.1", 0)).context("failed to bind udp probe")?;
    Ok(socket.local_addr()?.port())
}

pub fn wait_ready(fungi_dir: &Path, timeout: Duration) -> Result<()> {
    let fungi_bin = get_fungi_binary_path()?;
    wait_ready_with_bin(&fungi_bin, &std::env::current_dir()?, fungi_dir, timeout)
}

pub struct DaemonProcess {
    child: Child,
}

impl DaemonProcess {
    pub fn start(fungi_dir: &Path, timeout: Duration) -> Result<Self> {
        let fungi_bin = get_fungi_binary_path()?;
        let child = Command::new(&fungi_bin)
            .arg("--fungi-dir")
            .arg(fungi_dir)
            .arg("daemon")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("failed to start daemon")?;

        wait_ready(fungi_dir, timeout)?;
        Ok(Self { child })
    }
}

impl Drop for DaemonProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub struct RelayProcess {
    child: Child,
    peer_id: String,
    _stdout_drain: JoinHandle<()>,
}

impl RelayProcess {
    pub fn start(home_dir: &Path, tcp_port: u16, udp_port: u16, timeout: Duration) -> Result<Self> {
        let fungi_bin = get_fungi_binary_path()?;
        let mut child = Command::new(&fungi_bin)
            .env("HOME", home_dir)
            .arg("daemon")
            .arg("relay-server")
            .arg("--public-ip")
            .arg("127.0.0.1")
            .arg("--tcp-listen-port")
            .arg(tcp_port.to_string())
            .arg("--udp-listen-port")
            .arg(udp_port.to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("failed to start relay server")?;

        let stdout = child
            .stdout
            .take()
            .context("failed to capture relay stdout")?;
        let (peer_id, stdout_drain) = read_relay_peer_id(stdout, timeout)?;
        Ok(Self {
            child,
            peer_id,
            _stdout_drain: stdout_drain,
        })
    }

    pub fn peer_id(&self) -> &str {
        &self.peer_id
    }

    pub fn stop(&mut self) -> Result<()> {
        self.child.kill().context("failed to kill relay process")?;
        let _ = self.child.wait();
        Ok(())
    }
}

impl Drop for RelayProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn start_background_lab(args: StartArgs) -> Result<()> {
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

fn run_manager(args: ManagerArgs) -> Result<()> {
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

fn print_status(args: StatusArgs) -> Result<()> {
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

fn stop_default_lab() -> Result<()> {
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

fn clean_default_lab() -> Result<()> {
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

fn print_env() -> Result<()> {
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

fn manage_node(command: NodeCommand) -> Result<()> {
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

fn manage_relay(command: ProcessCommand) -> Result<()> {
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

fn apply_trust_mode_to_default_lab(mode: TrustMode) -> Result<()> {
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

fn stop_lab(state: &LabState) -> Result<()> {
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

fn wait_ready_with_bin(
    fungi_bin: &Path,
    repo: &Path,
    fungi_dir: &Path,
    timeout: Duration,
) -> Result<()> {
    let started = Instant::now();
    while started.elapsed() < timeout {
        let output = Command::new(fungi_bin)
            .current_dir(repo)
            .arg("--fungi-dir")
            .arg(fungi_dir)
            .arg("info")
            .arg("version")
            .output()
            .context("failed to probe daemon readiness")?;

        if output.status.success() {
            return Ok(());
        }

        thread::sleep(Duration::from_millis(300));
    }

    bail!("daemon did not become ready within {:?}", timeout)
}

fn wait_peer_id(
    fungi_bin: &Path,
    repo: &Path,
    fungi_dir: &Path,
    timeout: Duration,
) -> Result<String> {
    let started = Instant::now();
    let mut last = String::new();
    while started.elapsed() < timeout {
        let output = Command::new(fungi_bin)
            .current_dir(repo)
            .arg("--fungi-dir")
            .arg(fungi_dir)
            .arg("info")
            .arg("id")
            .output()
            .context("failed to query daemon peer id")?;
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Some(peer_id) = parse_peer_id(&stdout) {
                return Ok(peer_id);
            }
            last = stdout.to_string();
        } else {
            last = String::from_utf8_lossy(&output.stderr).to_string();
        }
        thread::sleep(Duration::from_millis(300));
    }
    bail!(
        "daemon RPC did not become ready for {}\n{}",
        fungi_dir.display(),
        last
    )
}

fn run_cli_capture<I, S>(
    fungi_bin: &Path,
    repo: &Path,
    fungi_dir: &Path,
    args: I,
    input: Option<&str>,
) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let output = run_cli_output(fungi_bin, repo, fungi_dir, args, input)?;
    if !output.status.success() {
        bail!(
            "fungi command failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn run_cli_status<I, S>(
    fungi_bin: &Path,
    repo: &Path,
    fungi_dir: &Path,
    args: I,
    input: Option<&str>,
) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let output = run_cli_output(fungi_bin, repo, fungi_dir, args, input)?;
    if !output.status.success() {
        bail!(
            "fungi command failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

fn run_cli_output<I, S>(
    fungi_bin: &Path,
    repo: &Path,
    fungi_dir: &Path,
    args: I,
    input: Option<&str>,
) -> Result<std::process::Output>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut command = Command::new(fungi_bin);
    command.current_dir(repo).arg("--fungi-dir").arg(fungi_dir);
    for arg in args {
        command.arg(arg.as_ref());
    }
    if input.is_some() {
        command.stdin(Stdio::piped());
    }
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().context("failed to run fungi command")?;
    if let Some(input) = input {
        use std::io::Write;
        let stdin = child
            .stdin
            .as_mut()
            .context("failed to open command stdin")?;
        stdin.write_all(input.as_bytes())?;
    }
    Ok(child.wait_with_output()?)
}

fn read_relay_peer_id(stdout: ChildStdout, timeout: Duration) -> Result<(String, JoinHandle<()>)> {
    let started = Instant::now();
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();

    while started.elapsed() < timeout {
        line.clear();
        let read = reader.read_line(&mut line)?;
        if read == 0 {
            thread::sleep(Duration::from_millis(100));
            continue;
        }
        if let Some(peer_id) = line.trim().strip_prefix("Local peer id: ") {
            let peer_id = peer_id.to_string();
            let stdout_drain = thread::spawn(move || {
                let mut discard = String::new();
                loop {
                    discard.clear();
                    match reader.read_line(&mut discard) {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {}
                    }
                }
            });
            return Ok((peer_id, stdout_drain));
        }
    }

    bail!("timed out waiting for relay peer id output")
}

fn wait_relay_peer_id_from_log(log: &Path, timeout: Duration) -> Result<String> {
    let started = Instant::now();
    let mut last = String::new();
    while started.elapsed() < timeout {
        if let Ok(contents) = fs::read_to_string(log) {
            if let Some(peer_id) = contents.lines().find_map(|line| {
                line.trim()
                    .strip_prefix("Local peer id: ")
                    .map(ToOwned::to_owned)
            }) {
                return Ok(peer_id);
            }
            last = contents
                .lines()
                .rev()
                .take(20)
                .collect::<Vec<_>>()
                .join("\n");
        }
        thread::sleep(Duration::from_millis(100));
    }
    bail!(
        "timed out waiting for relay peer id in {}\n{}",
        log.display(),
        last
    )
}

fn write_node_config(
    fungi_dir: &Path,
    rpc_port: u16,
    tcp_port: u16,
    udp_port: u16,
    relay_addrs: &[String],
) -> Result<()> {
    let relay_list = relay_addrs
        .iter()
        .map(|addr| format!("\"{addr}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let config = format!(
        "version = 2\n\n[rpc]\nlisten_address = \"127.0.0.1:{rpc_port}\"\n\n[network]\nlisten_tcp_port = {tcp_port}\nlisten_udp_port = {udp_port}\nrelay_enabled = true\nuse_community_relays = false\ncustom_relay_addresses = [{relay_list}]\n\n[runtime]\ndisable_docker = false\ndisable_wasmtime = false\n"
    );
    fs::write(fungi_dir.join("config.toml"), config).with_context(|| {
        format!(
            "failed to write {}",
            fungi_dir.join("config.toml").display()
        )
    })
}

fn wait_for_state(root: &Path, timeout: Duration) -> Result<LabState> {
    let started = Instant::now();
    let mut last_error = None;
    while started.elapsed() < timeout {
        match read_state(root) {
            Ok(state) if state.ready => return Ok(state),
            Ok(_) => {}
            Err(error) => last_error = Some(error.to_string()),
        }
        thread::sleep(Duration::from_millis(250));
    }
    bail!(
        "timed out waiting for local lab startup{}",
        last_error
            .map(|error| format!(": {error}"))
            .unwrap_or_default()
    )
}

fn read_state(root: &Path) -> Result<LabState> {
    let path = root.join(STATE_FILE);
    let raw =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    Ok(serde_json::from_str(&raw)?)
}

fn write_state(state: &LabState) -> Result<()> {
    fs::create_dir_all(&state.root)?;
    let path = state.root.join(STATE_FILE);
    let raw = serde_json::to_string_pretty(state)?;
    fs::write(&path, raw).with_context(|| format!("failed to write {}", path.display()))
}

fn stop_pid(pid: Option<u32>, spec: &ProcessSpec, force: bool) -> Result<()> {
    let Some(pid) = pid else {
        return Ok(());
    };
    let Some(system) = system_with_process(pid) else {
        return Ok(());
    };
    if !process_matches(&system, pid, spec) {
        bail!(
            "refusing to stop pid {pid}: process does not match expected {} lab process",
            spec.label
        );
    }

    if let Some(process) = system.process(Pid::from_u32(pid)) {
        let _ = process
            .kill_with(Signal::Term)
            .unwrap_or_else(|| process.kill());
    }

    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if !process_is_running(Some(pid), spec) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }

    if force
        && let Some(system) = system_with_process(pid)
        && let Some(process) = system.process(Pid::from_u32(pid))
    {
        let _ = process.kill();
    }

    Ok(())
}

fn process_is_running(pid: Option<u32>, spec: &ProcessSpec) -> bool {
    let Some(pid) = pid else {
        return false;
    };
    system_with_process(pid)
        .map(|system| process_matches(&system, pid, spec))
        .unwrap_or(false)
}

fn system_with_process(pid: u32) -> Option<System> {
    let system = System::new_all();
    if system.process(Pid::from_u32(pid)).is_some() {
        Some(system)
    } else {
        None
    }
}

fn process_matches(system: &System, pid: u32, spec: &ProcessSpec) -> bool {
    let Some(process) = system.process(Pid::from_u32(pid)) else {
        return false;
    };
    process_matches_exe(process, spec) && process_matches_cmd(process, spec)
}

fn process_matches_exe(process: &Process, spec: &ProcessSpec) -> bool {
    let Some(expected_exe) = &spec.exe else {
        return true;
    };
    let Some(actual_exe) = process.exe() else {
        return false;
    };
    paths_match(actual_exe, expected_exe)
}

fn process_matches_cmd(process: &Process, spec: &ProcessSpec) -> bool {
    let cmd = process
        .cmd()
        .iter()
        .map(|part| part.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ");
    spec.cmd_contains
        .iter()
        .all(|expected| cmd.contains(expected))
}

fn paths_match(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

fn default_root() -> Result<PathBuf> {
    Ok(find_repo_root()?.join("target/local-lab"))
}

fn find_repo_root() -> Result<PathBuf> {
    let mut candidates = Vec::new();
    candidates.push(std::env::current_dir()?);
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            candidates.push(parent.to_path_buf());
        }
    }
    for start in candidates {
        for path in start.ancestors() {
            if path.join("Cargo.toml").exists() && path.join("fungi/Cargo.toml").exists() {
                return Ok(path.to_path_buf());
            }
        }
    }
    bail!("could not find fungi repo root")
}

fn print_started_summary(state: &LabState) {
    println!("Fungi local lab started.");
    println!("  node-a: {}", state.node_a.dir.display());
    println!("  node-b: {}", state.node_b.dir.display());
    println!("  relay:  {}", state.root.display());
    println!("  A peer: {}", state.node_a.peer_id);
    println!("  B peer: {}", state.node_b.peer_id);
    println!();
    println!(
        "Use: ./target/debug/fungi -f {} service list",
        display_path_arg(&state.repo, &state.node_a.dir)
    );
    println!(
        "Use: ./target/debug/fungi -f {} service list",
        display_path_arg(&state.repo, &state.node_b.dir)
    );
}

fn print_process(
    name: &str,
    pid: Option<u32>,
    peer_id: Option<&str>,
    log: Option<&Path>,
    spec: &ProcessSpec,
) {
    let state = if process_is_running(pid, spec) {
        "running"
    } else {
        "stopped"
    };
    println!(
        "  {name}: {state} pid={}",
        pid.map_or("-".to_string(), |pid| pid.to_string())
    );
    if let Some(peer_id) = peer_id {
        println!("    peer_id: {peer_id}");
    }
    if let Some(log) = log {
        println!("    log: {}", log.display());
    }
}

fn parse_peer_id(text: &str) -> Option<String> {
    text.split_whitespace()
        .find(|part| part.starts_with("16Uiu"))
        .map(ToOwned::to_owned)
}

fn circuit_addr(relay_tcp_addr: &str, peer_id: &str) -> String {
    format!("{relay_tcp_addr}/p2p-circuit/p2p/{peer_id}")
}

fn epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn shell_quote(value: impl AsRef<str>) -> String {
    let value = value.as_ref();
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn shell_quote_path(value: &Path) -> String {
    shell_quote(value.display().to_string())
}

fn display_path_arg<'a>(repo: &'a Path, path: &'a Path) -> std::path::Display<'a> {
    path.strip_prefix(repo).unwrap_or(path).display()
}

fn detach_process_group(command: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(not(unix))]
    {
        let _ = command;
    }
}

impl TrustMode {
    fn as_arg(self) -> &'static str {
        match self {
            TrustMode::Both => "both",
            TrustMode::BTrustsA => "b-trusts-a",
            TrustMode::ATrustsB => "a-trusts-b",
            TrustMode::None => "none",
        }
    }
}

impl NodeCommand {
    fn node(self) -> NodeName {
        match self {
            NodeCommand::Start { node }
            | NodeCommand::Stop { node }
            | NodeCommand::Restart { node } => node,
        }
    }
}

impl NodeState {
    fn empty(name: &str, dir: PathBuf) -> Self {
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
    fn node(&self, node: NodeName) -> &NodeState {
        match node {
            NodeName::A => &self.node_a,
            NodeName::B => &self.node_b,
        }
    }

    fn node_mut(&mut self, node: NodeName) -> &mut NodeState {
        match node {
            NodeName::A => &mut self.node_a,
            NodeName::B => &mut self.node_b,
        }
    }

    fn process_spec_for_manager(&self) -> ProcessSpec {
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

    fn process_spec_for_relay(&self) -> ProcessSpec {
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

    fn process_spec_for_node(&self, node: NodeName) -> ProcessSpec {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_node_config_disables_community_relays() {
        let dir = tempfile::tempdir().unwrap();
        write_node_config(
            dir.path(),
            1111,
            2222,
            3333,
            &["/ip4/127.0.0.1/tcp/4444/p2p/relay".to_string()],
        )
        .unwrap();

        let content = fs::read_to_string(dir.path().join("config.toml")).unwrap();
        assert!(content.contains("listen_address = \"127.0.0.1:1111\""));
        assert!(content.contains("listen_tcp_port = 2222"));
        assert!(content.contains("listen_udp_port = 3333"));
        assert!(content.contains("relay_enabled = true"));
        assert!(content.contains("use_community_relays = false"));
        assert!(content.contains("/ip4/127.0.0.1/tcp/4444/p2p/relay"));
    }

    #[test]
    fn peer_id_parser_finds_libp2p_peer_id() {
        let peer_id = "16Uiu2HAmGXFS6aYsKKYRkEDo1tNigZKN8TAYrsfSnEdC5sZLNkiE";
        assert_eq!(
            parse_peer_id(&format!("Local Peer ID: {peer_id}")),
            Some(peer_id.to_string())
        );
    }
}
