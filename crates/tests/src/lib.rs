use anyhow::{Context, Result, bail};
use std::fs;
use std::io::{BufRead, BufReader};
use std::net::{TcpListener, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::thread;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

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
    let started = Instant::now();

    while started.elapsed() < timeout {
        let output = Command::new(&fungi_bin)
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
