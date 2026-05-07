use anyhow::{Context, Result, bail};
use std::{
    fs,
    io::{BufRead, BufReader},
    net::{TcpListener, UdpSocket},
    path::{Path, PathBuf},
    process::{Child, ChildStdout, Command, Stdio},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use sysinfo::{Pid, Process, Signal, System};

use crate::state::ProcessSpec;
use crate::util::wait_ready_with_bin;

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

pub(crate) fn stop_pid(pid: Option<u32>, spec: &ProcessSpec, force: bool) -> Result<()> {
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

pub(crate) fn process_is_running(pid: Option<u32>, spec: &ProcessSpec) -> bool {
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

pub(crate) fn detach_process_group(command: &mut Command) {
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
