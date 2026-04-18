use anyhow::{Context, Result, bail};
use fungi_tests::{
    DaemonProcess, RelayProcess, assert_contains, get_fungi_binary_path, init_fungi_dir,
    patch_rpc_port, reserve_tcp_port, reserve_udp_port,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

const STABILITY_WAIT: Duration = Duration::from_secs(10);
const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
const RECOVERY_TIMEOUT: Duration = Duration::from_secs(45);

fn main() -> Result<()> {
    let ctx = RelayRuntimeTestContext::new()?;

    println!("=== Start local relay ===");
    let mut relay = RelayProcess::start(
        ctx.relay_home(),
        ctx.relay_tcp_port,
        ctx.relay_udp_port,
        STARTUP_TIMEOUT,
    )?;

    println!("=== Configure daemon relay addresses ===");
    ctx.configure_daemon_relay(relay.peer_id())?;

    println!("=== Start local daemon ===");
    let daemon = DaemonProcess::start(ctx.fungi_dir(), STARTUP_TIMEOUT)?;

    println!("=== Wait for relay reservation ===");
    let first_connection_id =
        wait_for_active_relay_connection(&ctx, relay.peer_id(), STARTUP_TIMEOUT)?;
    println!("Active relay connection id: {first_connection_id}");

    println!("=== Check relay stability before restart ===");
    thread::sleep(STABILITY_WAIT);
    let second_connection_id =
        wait_for_active_relay_connection(&ctx, relay.peer_id(), Duration::from_secs(10))?;
    if second_connection_id != first_connection_id {
        bail!(
            "relay connection changed across health interval: {} -> {}",
            first_connection_id,
            second_connection_id
        );
    }
    println!("Relay connection stayed stable before restart.");

    println!("=== Restart relay and verify recovery ===");
    relay.stop()?;
    thread::sleep(Duration::from_secs(2));
    relay = RelayProcess::start(
        ctx.relay_home(),
        ctx.relay_tcp_port,
        ctx.relay_udp_port,
        STARTUP_TIMEOUT,
    )?;

    let recovered_connection_id =
        wait_for_active_relay_connection(&ctx, relay.peer_id(), RECOVERY_TIMEOUT)?;
    println!("Recovered relay connection id: {recovered_connection_id}");

    let overview = ctx.run_cli([
        "connection",
        "overview",
        "--peer-id",
        relay.peer_id(),
        "--verbose",
    ])?;
    assert_contains(&overview, &recovered_connection_id)?;

    drop(daemon);
    drop(relay);
    println!("Relay runtime CLI smoke test passed.");
    Ok(())
}

struct RelayRuntimeTestContext {
    _temp_dir: TempDir,
    fungi_dir: PathBuf,
    relay_home: PathBuf,
    relay_tcp_port: u16,
    relay_udp_port: u16,
}

impl RelayRuntimeTestContext {
    fn new() -> Result<Self> {
        let temp_dir = TempDir::new().context("failed to create temp dir")?;
        let fungi_dir = temp_dir.path().join("fungi-home");
        let relay_home = temp_dir.path().join("relay-home");
        fs::create_dir_all(&fungi_dir)?;
        fs::create_dir_all(&relay_home)?;

        init_fungi_dir(&fungi_dir)?;

        let rpc_port = reserve_tcp_port()?;
        let relay_tcp_port = reserve_tcp_port()?;
        let relay_udp_port = reserve_udp_port()?;
        patch_rpc_port(&fungi_dir.join("config.toml"), rpc_port)?;

        Ok(Self {
            _temp_dir: temp_dir,
            fungi_dir,
            relay_home,
            relay_tcp_port,
            relay_udp_port,
        })
    }

    fn fungi_dir(&self) -> &Path {
        &self.fungi_dir
    }

    fn relay_home(&self) -> &Path {
        &self.relay_home
    }

    fn configure_daemon_relay(&self, relay_peer_id: &str) -> Result<()> {
        let config_file = self.fungi_dir.join("config.toml");
        let mut content = fs::read_to_string(&config_file)
            .with_context(|| format!("failed to read {}", config_file.display()))?;

        if !content.contains("use_community_relays = false") {
            content = content.replace(
                "use_community_relays = true",
                "use_community_relays = false",
            );
        }

        let relay_tcp = format!(
            "\"/ip4/127.0.0.1/tcp/{}/p2p/{}\"",
            self.relay_tcp_port, relay_peer_id
        );
        let relay_udp = format!(
            "\"/ip4/127.0.0.1/udp/{}/quic-v1/p2p/{}\"",
            self.relay_udp_port, relay_peer_id
        );

        if let Some(start) = content.find("custom_relay_addresses = [") {
            let end = content[start..]
                .find(']')
                .map(|offset| start + offset)
                .context("failed to find end of custom_relay_addresses")?;
            let replacement = format!("custom_relay_addresses = [{}, {}]", relay_tcp, relay_udp);
            content.replace_range(start..=end, &replacement);
        } else {
            bail!("custom_relay_addresses field not found in config file");
        }

        fs::write(&config_file, content)
            .with_context(|| format!("failed to write {}", config_file.display()))?;
        Ok(())
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
            .with_context(|| format!("failed to run cli command {:?}", arg_list))?;

        if !output.status.success() {
            bail!(
                "command {:?} failed\nstdout:\n{}\nstderr:\n{}",
                arg_list,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            );
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}

fn wait_for_active_relay_connection(
    ctx: &RelayRuntimeTestContext,
    relay_peer_id: &str,
    timeout: Duration,
) -> Result<String> {
    let started = Instant::now();
    let mut last_status_output = String::new();
    let mut last_overview_output = String::new();
    while started.elapsed() < timeout {
        let status_output = ctx.run_cli(["connection", "relay-status", "--verbose"])?;
        if let Some(connection_id) = parse_active_direct_connection(&status_output, relay_peer_id) {
            return Ok(connection_id);
        }

        let overview_output = ctx.run_cli([
            "connection",
            "overview",
            "--peer-id",
            relay_peer_id,
            "--verbose",
        ])?;
        if let Some(connection_id) = parse_connection_overview(&overview_output, relay_peer_id) {
            return Ok(connection_id);
        }

        last_status_output = status_output;
        last_overview_output = overview_output;
        thread::sleep(Duration::from_secs(2));
    }

    bail!(
        "timed out waiting for active relay connection for peer {}\nrelay-status:\n{}\n\nconnection overview:\n{}",
        relay_peer_id,
        last_status_output,
        last_overview_output,
    )
}

fn parse_active_direct_connection(output: &str, relay_peer_id: &str) -> Option<String> {
    for line in output.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("peer_id=") || !trimmed.contains(relay_peer_id) {
            continue;
        }

        let direct_part = trimmed
            .split_whitespace()
            .find(|part| part.starts_with("direct_conn="))?;
        let connection_id = direct_part.strip_prefix("direct_conn=")?;
        if connection_id != "-" {
            return Some(connection_id.to_string());
        }
    }

    None
}

fn parse_connection_overview(output: &str, relay_peer_id: &str) -> Option<String> {
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with("Connection overview")
            || trimmed.starts_with("PEER")
            || !trimmed.starts_with(relay_peer_id)
        {
            continue;
        }

        let mut parts = trimmed.split_whitespace();
        let peer = parts.next()?;
        let connection_id = parts.next()?;
        let _direction = parts.next()?;
        let _is_relay = parts.next()?;
        if peer == relay_peer_id {
            return Some(connection_id.to_string());
        }
    }

    None
}
