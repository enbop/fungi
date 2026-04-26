use anyhow::{Context, Result, bail};
use fungi_tests::{
    RelayProcess, get_fungi_binary_path, init_fungi_dir, reserve_tcp_port, reserve_udp_port,
    wait_ready,
};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tempfile::TempDir;

const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
const PING_SUCCESS_TIMEOUT: Duration = Duration::from_secs(20);
const PING_INACTIVE_TIMEOUT: Duration = Duration::from_secs(8);
const PING_INTERVAL_MS: &str = "1000";

fn main() -> Result<()> {
    run_scenario(
        "no-allowlist",
        false,
        false,
        &[
            ScenarioPingCheck::new(NodeSide::A, NodeSide::B, PingExpectation::Inactive),
            ScenarioPingCheck::new(NodeSide::B, NodeSide::A, PingExpectation::Inactive),
        ],
    )?;
    run_scenario(
        "only-node-b-allows-node-a",
        false,
        true,
        &[
            ScenarioPingCheck::new(NodeSide::B, NodeSide::A, PingExpectation::Inactive),
            ScenarioPingCheck::new(NodeSide::A, NodeSide::B, PingExpectation::Succeeds),
        ],
    )?;
    run_scenario(
        "only-node-a-allows-node-b",
        true,
        false,
        &[
            ScenarioPingCheck::new(NodeSide::A, NodeSide::B, PingExpectation::Inactive),
            ScenarioPingCheck::new(NodeSide::B, NodeSide::A, PingExpectation::Succeeds),
        ],
    )?;
    run_scenario(
        "both-nodes-allow-each-other",
        true,
        true,
        &[
            ScenarioPingCheck::new(NodeSide::A, NodeSide::B, PingExpectation::Succeeds),
            ScenarioPingCheck::new(NodeSide::B, NodeSide::A, PingExpectation::Succeeds),
        ],
    )?;

    println!("Allowlist relay ping CLI smoke test passed.");
    Ok(())
}

fn run_scenario(
    name: &str,
    node_a_allows_b: bool,
    node_b_allows_a: bool,
    checks: &[ScenarioPingCheck],
) -> Result<()> {
    println!("\n=== Scenario: {name} ===");
    let scenario = AllowlistRelayScenario::start(name)?;

    if node_a_allows_b {
        println!("Configuring node-a to allow node-b");
        scenario.node_a.allow_peer(&scenario.node_b_peer_id)?;
    }
    if node_b_allows_a {
        println!("Configuring node-b to allow node-a");
        scenario.node_b.allow_peer(&scenario.node_a_peer_id)?;
    }

    thread::sleep(Duration::from_millis(500));

    for check in checks {
        scenario.assert_ping(*check)?;
    }

    println!("Scenario {name} completed.");
    Ok(())
}

#[derive(Clone, Copy, Debug)]
enum PingExpectation {
    Succeeds,
    Inactive,
}

#[derive(Clone, Copy, Debug)]
enum NodeSide {
    A,
    B,
}

#[derive(Clone, Copy, Debug)]
struct ScenarioPingCheck {
    source: NodeSide,
    target: NodeSide,
    expectation: PingExpectation,
}

impl ScenarioPingCheck {
    const fn new(source: NodeSide, target: NodeSide, expectation: PingExpectation) -> Self {
        Self {
            source,
            target,
            expectation,
        }
    }
}

struct AllowlistRelayScenario {
    _temp_dir: TempDir,
    _relay: RelayProcess,
    node_a: TestNode,
    node_b: TestNode,
    node_a_peer_id: String,
    node_b_peer_id: String,
}

impl AllowlistRelayScenario {
    fn start(name: &str) -> Result<Self> {
        let temp_dir = TempDir::new().context("failed to create temp dir")?;
        let relay_home = temp_dir.path().join("relay-home");
        fs::create_dir_all(&relay_home).context("failed to create relay home")?;

        let relay_tcp_port = reserve_tcp_port()?;
        let relay_udp_port = reserve_udp_port()?;
        let relay =
            RelayProcess::start(&relay_home, relay_tcp_port, relay_udp_port, STARTUP_TIMEOUT)?;

        let node_a_dir = temp_dir.path().join(format!("{name}-node-a"));
        let node_b_dir = temp_dir.path().join(format!("{name}-node-b"));
        let node_a = TestNode::start(
            "node-a",
            &node_a_dir,
            relay.peer_id(),
            relay_tcp_port,
            relay_udp_port,
        )?;
        let node_b = TestNode::start(
            "node-b",
            &node_b_dir,
            relay.peer_id(),
            relay_tcp_port,
            relay_udp_port,
        )?;

        let node_a_peer_id = node_a.info_id()?;
        let node_b_peer_id = node_b.info_id()?;

        node_a.device_add(&node_b_peer_id, "node-b")?;
        node_b.device_add(&node_a_peer_id, "node-a")?;

        Ok(Self {
            _temp_dir: temp_dir,
            _relay: relay,
            node_a,
            node_b,
            node_a_peer_id,
            node_b_peer_id,
        })
    }

    fn assert_ping(&self, check: ScenarioPingCheck) -> Result<()> {
        let source = self.node(check.source);
        let target = self.node(check.target);
        let label = format!(
            "{} -> {}",
            self.side_name(check.source),
            self.side_name(check.target)
        );
        let peer_id = self.peer_id(check.target);

        println!("Checking {label} ({:?})", check.expectation);
        let capture = source.capture_ping(peer_id, check.expectation.timeout())?;

        match check.expectation {
            PingExpectation::Succeeds => {
                if capture.saw_rtt {
                    return Ok(());
                }
                bail!(
                    "{label} did not produce an RTT within {:?}\n{}\n{}",
                    check.expectation.timeout(),
                    capture.describe("ping output"),
                    self.diagnostics_for(source, target, peer_id),
                );
            }
            PingExpectation::Inactive => {
                if capture.saw_rtt {
                    bail!(
                        "{label} unexpectedly produced an RTT\n{}\n{}",
                        capture.describe("ping output"),
                        self.diagnostics_for(source, target, peer_id),
                    );
                }
                if capture.saw_inactive {
                    return Ok(());
                }
                bail!(
                    "{label} never reported inactivity or an error\n{}\n{}",
                    capture.describe("ping output"),
                    self.diagnostics_for(source, target, peer_id),
                );
            }
        }
    }

    fn node(&self, side: NodeSide) -> &TestNode {
        match side {
            NodeSide::A => &self.node_a,
            NodeSide::B => &self.node_b,
        }
    }

    fn peer_id(&self, side: NodeSide) -> &str {
        match side {
            NodeSide::A => &self.node_a_peer_id,
            NodeSide::B => &self.node_b_peer_id,
        }
    }

    fn side_name(&self, side: NodeSide) -> &'static str {
        match side {
            NodeSide::A => "node-a",
            NodeSide::B => "node-b",
        }
    }

    fn diagnostics_for(
        &self,
        source: &TestNode,
        target: &TestNode,
        target_peer_id: &str,
    ) -> String {
        format!(
            "{}\n{}\n{}\n{}",
            source.connection_overview(target_peer_id),
            target.connection_overview(&source.info_id().unwrap_or_default()),
            source.tail_log(80, "source daemon log"),
            target.tail_log(80, "target daemon log")
        )
    }
}

impl PingExpectation {
    fn timeout(self) -> Duration {
        match self {
            PingExpectation::Succeeds => PING_SUCCESS_TIMEOUT,
            PingExpectation::Inactive => PING_INACTIVE_TIMEOUT,
        }
    }
}

struct TestNode {
    name: String,
    fungi_dir: PathBuf,
    log_file: PathBuf,
    child: Child,
}

impl TestNode {
    fn start(
        name: &str,
        fungi_dir: &Path,
        relay_peer_id: &str,
        relay_tcp_port: u16,
        relay_udp_port: u16,
    ) -> Result<Self> {
        fs::create_dir_all(fungi_dir)
            .with_context(|| format!("failed to create fungi dir for {name}"))?;
        init_fungi_dir(fungi_dir)?;

        let rpc_port = reserve_tcp_port()?;
        let listen_tcp_port = reserve_tcp_port()?;
        let listen_udp_port = reserve_udp_port()?;
        write_test_config(
            fungi_dir,
            rpc_port,
            listen_tcp_port,
            listen_udp_port,
            relay_peer_id,
            relay_tcp_port,
            relay_udp_port,
        )?;

        let fungi_bin = get_fungi_binary_path()?;
        let log_file = fungi_dir.join(format!("{name}-daemon.log"));
        let stdout = File::create(&log_file)
            .with_context(|| format!("failed to create log file for {name}"))?;
        let stderr = stdout
            .try_clone()
            .with_context(|| format!("failed to clone log file for {name}"))?;

        let child = Command::new(&fungi_bin)
            .arg("--fungi-dir")
            .arg(fungi_dir)
            .arg("daemon")
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr))
            .spawn()
            .with_context(|| format!("failed to start daemon for {name}"))?;

        wait_ready(fungi_dir, STARTUP_TIMEOUT)?;

        Ok(Self {
            name: name.to_string(),
            fungi_dir: fungi_dir.to_path_buf(),
            log_file,
            child,
        })
    }

    fn run_cli<I, S>(&self, args: I) -> Result<String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let fungi_bin = get_fungi_binary_path()?;
        let arg_list = args
            .into_iter()
            .map(|entry| entry.as_ref().to_string())
            .collect::<Vec<_>>();

        let output = Command::new(&fungi_bin)
            .arg("--fungi-dir")
            .arg(&self.fungi_dir)
            .args(&arg_list)
            .output()
            .with_context(|| format!("failed to run cli on node {}", self.name))?;

        if !output.status.success() {
            bail!(
                "node {} command {:?} failed\nstdout:\n{}\nstderr:\n{}\n{}",
                self.name,
                arg_list,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
                self.tail_log(120, "daemon log tail"),
            );
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn info_id(&self) -> Result<String> {
        self.run_cli(["info", "id"])
    }

    fn device_add(&self, peer_id: &str, name: &str) -> Result<()> {
        let output = self.run_cli(["device", "add", peer_id, "--name", name])?;
        if !output.contains("Device saved") {
            bail!("unexpected device add output on {}: {output}", self.name);
        }
        Ok(())
    }

    fn allow_peer(&self, peer_id: &str) -> Result<()> {
        let output = self.run_cli(["security", "allowed-peers", "add", peer_id])?;
        if !output.contains("Peer added successfully") {
            bail!("unexpected allow-peer output on {}: {output}", self.name);
        }
        Ok(())
    }

    fn capture_ping(&self, peer_id: &str, timeout: Duration) -> Result<PingCapture> {
        let fungi_bin = get_fungi_binary_path()?;
        let mut child = Command::new(&fungi_bin)
            .arg("--fungi-dir")
            .arg(&self.fungi_dir)
            .arg("ping")
            .arg(peer_id)
            .arg("--interval-ms")
            .arg(PING_INTERVAL_MS)
            .arg("--verbose")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("failed to start ping command on {}", self.name))?;

        let stdout = child
            .stdout
            .take()
            .context("failed to capture ping stdout")?;
        let stderr = child
            .stderr
            .take()
            .context("failed to capture ping stderr")?;

        let (tx, rx) = mpsc::channel::<String>();
        let stdout_handle = spawn_reader("stdout", stdout, tx.clone());
        let stderr_handle = spawn_reader("stderr", stderr, tx);
        let started = Instant::now();

        let mut capture = PingCapture::default();
        while started.elapsed() < timeout {
            if let Some(status) = child.try_wait()? {
                capture
                    .lines
                    .push(format!("[process exited with status {status}]"));
                break;
            }

            match rx.recv_timeout(Duration::from_millis(250)) {
                Ok(line) => {
                    capture.observe(line);
                    if capture.saw_rtt {
                        break;
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }

        let _ = child.kill();
        let _ = child.wait();

        while let Ok(line) = rx.recv_timeout(Duration::from_millis(20)) {
            capture.observe(line);
        }

        stdout_handle.join().ok();
        stderr_handle.join().ok();

        Ok(capture)
    }

    fn connection_overview(&self, peer_id: &str) -> String {
        match self.run_cli(["connection", "overview", "--peer-id", peer_id, "--verbose"]) {
            Ok(output) => format!("== {} connection overview ==\n{}", self.name, output),
            Err(error) => format!(
                "== {} connection overview ==\n<failed: {}>",
                self.name, error
            ),
        }
    }

    fn tail_log(&self, lines: usize, label: &str) -> String {
        let Ok(contents) = fs::read_to_string(&self.log_file) else {
            return format!(
                "== {} {} ==\n<failed to read {}>",
                self.name,
                label,
                self.log_file.display()
            );
        };
        let all_lines = contents.lines().collect::<Vec<_>>();
        let start = all_lines.len().saturating_sub(lines);
        format!(
            "== {} {} ==\n{}",
            self.name,
            label,
            all_lines[start..].join("\n")
        )
    }
}

impl Drop for TestNode {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[derive(Default)]
struct PingCapture {
    lines: Vec<String>,
    saw_rtt: bool,
    saw_inactive: bool,
}

impl PingCapture {
    fn observe(&mut self, line: String) {
        if line.contains("rtt=") {
            self.saw_rtt = true;
        }
        if line.contains("no active connections") || line.contains("error=") {
            self.saw_inactive = true;
        }
        self.lines.push(line);
    }

    fn describe(&self, label: &str) -> String {
        if self.lines.is_empty() {
            return format!("== {label} ==\n<no output captured>");
        }

        format!("== {label} ==\n{}", self.lines.join("\n"))
    }
}

fn write_test_config(
    fungi_dir: &Path,
    rpc_port: u16,
    listen_tcp_port: u16,
    listen_udp_port: u16,
    relay_peer_id: &str,
    relay_tcp_port: u16,
    relay_udp_port: u16,
) -> Result<()> {
    let relay_tcp = format!("/ip4/127.0.0.1/tcp/{relay_tcp_port}/p2p/{relay_peer_id}");
    let relay_udp = format!("/ip4/127.0.0.1/udp/{relay_udp_port}/quic-v1/p2p/{relay_peer_id}");
    let config = format!(
        "[rpc]\nlisten_address = \"127.0.0.1:{rpc_port}\"\n\n[network]\nlisten_tcp_port = {listen_tcp_port}\nlisten_udp_port = {listen_udp_port}\nrelay_enabled = true\nuse_community_relays = false\ncustom_relay_addresses = [\"{relay_tcp}\", \"{relay_udp}\"]\nincoming_allowed_peers = []\n"
    );

    fs::write(fungi_dir.join("config.toml"), config).with_context(|| {
        format!(
            "failed to write {}",
            fungi_dir.join("config.toml").display()
        )
    })?;
    Ok(())
}

fn spawn_reader<R>(label: &'static str, reader: R, tx: mpsc::Sender<String>) -> JoinHandle<()>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut reader = BufReader::new(reader);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let trimmed = line.trim_end().to_string();
                    if trimmed.is_empty() {
                        continue;
                    }
                    let _ = tx.send(format!("{label}: {trimmed}"));
                }
            }
        }
    })
}
