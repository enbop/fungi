use anyhow::{Context, Result, bail};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use crate::process::process_is_running;
use crate::state::{LabState, ProcessSpec};

pub(crate) const STATE_FILE: &str = "state.json";

pub(crate) fn wait_ready_with_bin(
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

pub(crate) fn wait_peer_id(
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

pub(crate) fn run_cli_capture<I, S>(
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

pub(crate) fn run_cli_status<I, S>(
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

pub(crate) fn run_cli_output<I, S>(
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

pub(crate) fn wait_relay_peer_id_from_log(log: &Path, timeout: Duration) -> Result<String> {
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

pub(crate) fn write_node_config(
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

pub(crate) fn wait_for_state(root: &Path, timeout: Duration) -> Result<LabState> {
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

pub(crate) fn read_state(root: &Path) -> Result<LabState> {
    let path = root.join(STATE_FILE);
    let raw =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    Ok(serde_json::from_str(&raw)?)
}

pub(crate) fn write_state(state: &LabState) -> Result<()> {
    fs::create_dir_all(&state.root)?;
    let path = state.root.join(STATE_FILE);
    let raw = serde_json::to_string_pretty(state)?;
    fs::write(&path, raw).with_context(|| format!("failed to write {}", path.display()))
}

pub(crate) fn default_root() -> Result<PathBuf> {
    Ok(find_repo_root()?.join("target/local-lab"))
}

pub(crate) fn find_repo_root() -> Result<PathBuf> {
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

pub(crate) fn print_started_summary(state: &LabState) {
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

pub(crate) fn print_process(
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

pub(crate) fn parse_peer_id(text: &str) -> Option<String> {
    text.split_whitespace()
        .find(|part| part.starts_with("16Uiu"))
        .map(ToOwned::to_owned)
}

pub(crate) fn circuit_addr(relay_tcp_addr: &str, peer_id: &str) -> String {
    format!("{relay_tcp_addr}/p2p-circuit/p2p/{peer_id}")
}

pub(crate) fn epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(crate) fn shell_quote(value: impl AsRef<str>) -> String {
    let value = value.as_ref();
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

pub(crate) fn shell_quote_path(value: &Path) -> String {
    shell_quote(value.display().to_string())
}

pub(crate) fn display_path_arg<'a>(repo: &'a Path, path: &'a Path) -> std::path::Display<'a> {
    path.strip_prefix(repo).unwrap_or(path).display()
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
