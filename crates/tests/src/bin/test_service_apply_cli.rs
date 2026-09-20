use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::{Duration, Instant};
use std::time::{SystemTime, UNIX_EPOCH};

const LOCAL_SERVICE: &str = "lab-local-apply-a";
const REMOTE_SERVICE: &str = "lab-remote-apply-b";
const TCP_SERVICE: &str = "lab-host-tcp";

fn main() -> Result<()> {
    env_logger::init();

    let repo = workspace_root()?;
    let fungi_bin = sibling_binary("fungi")?;
    let fungi_lab_bin = sibling_binary("fungi-lab")?;
    let manifests = tempfile::tempdir()?;
    let manifests_dir = manifests.path();
    let v1 = compile_wasi_fixture(&repo, manifests_dir, "v1")?;
    let v2 = compile_wasi_fixture(&repo, manifests_dir, "v2")?;
    let fixture_server = FixtureServer::start(v1, v2)?;

    let local_v1 = write_manifest(
        manifests_dir,
        "local-v1.yaml",
        "wasi-fixture",
        "http",
        &fixture_server.url("v1.wasm"),
        "/workspace",
    )?;
    let local_v2 = write_manifest(
        manifests_dir,
        "local-v2.yaml",
        "wasi-fixture",
        "http",
        &fixture_server.url("v2.wasm"),
        "/data",
    )?;
    let local_v2_web = write_manifest(
        manifests_dir,
        "local-v2-web.yaml",
        "wasi-fixture",
        "web",
        &fixture_server.url("v2.wasm"),
        "/data",
    )?;

    let lab_dir = create_lab_dir(&repo)?;
    let _cleanup = CleanupGuard {
        fungi_bin: fungi_bin.clone(),
        fungi_lab_bin: fungi_lab_bin.clone(),
        lab_dir: lab_dir.clone(),
    };

    run_lab(&fungi_lab_bin, &lab_dir, ["start", "--trust", "both"])?;

    let node_a = lab_dir.join("nodes/a/fungi");
    let node_b = lab_dir.join("nodes/b/fungi");

    println!("\n=== Local apply lifecycle ===");
    apply_service(&fungi_bin, &node_a, LOCAL_SERVICE, &local_v1)?;
    assert_service(&fungi_bin, &node_a, LOCAL_SERVICE, false, &["http"])?;
    start_service(&fungi_bin, &node_a, LOCAL_SERVICE)?;
    wait_for_log(&fungi_bin, &node_a, LOCAL_SERVICE, "fixture-v1")?;
    apply_service(&fungi_bin, &node_a, LOCAL_SERVICE, &local_v2)?;
    assert_service(&fungi_bin, &node_a, LOCAL_SERVICE, true, &["http"])?;
    apply_service(&fungi_bin, &node_a, LOCAL_SERVICE, &local_v2)?;
    assert_service(&fungi_bin, &node_a, LOCAL_SERVICE, true, &["http"])?;

    println!("\n=== Local stopped update and entry replacement ===");
    stop_service(&fungi_bin, &node_a, LOCAL_SERVICE)?;
    apply_service(&fungi_bin, &node_a, LOCAL_SERVICE, &local_v1)?;
    assert_service(&fungi_bin, &node_a, LOCAL_SERVICE, false, &["http"])?;
    start_service(&fungi_bin, &node_a, LOCAL_SERVICE)?;
    apply_service(&fungi_bin, &node_a, LOCAL_SERVICE, &local_v2_web)?;
    assert_service(&fungi_bin, &node_a, LOCAL_SERVICE, true, &["web"])?;
    apply_service(&fungi_bin, &node_a, LOCAL_SERVICE, &local_v2)?;
    assert_service(&fungi_bin, &node_a, LOCAL_SERVICE, true, &["http"])?;

    println!("\n=== Remote apply lifecycle ===");
    apply_service(
        &fungi_bin,
        &node_a,
        &format!("{REMOTE_SERVICE}@b"),
        &local_v1,
    )?;
    assert_service(&fungi_bin, &node_b, REMOTE_SERVICE, false, &["http"])?;
    start_service(&fungi_bin, &node_a, &format!("{REMOTE_SERVICE}@b"))?;
    wait_for_log(
        &fungi_bin,
        &node_a,
        &format!("{REMOTE_SERVICE}@b"),
        "fixture-v1",
    )?;
    apply_service(
        &fungi_bin,
        &node_a,
        &format!("{REMOTE_SERVICE}@b"),
        &local_v2,
    )?;
    assert_service(&fungi_bin, &node_b, REMOTE_SERVICE, true, &["http"])?;
    apply_service(
        &fungi_bin,
        &node_a,
        &format!("{REMOTE_SERVICE}@b"),
        &local_v2,
    )?;
    assert_service(&fungi_bin, &node_b, REMOTE_SERVICE, true, &["http"])?;

    println!("\n=== Remote stopped update and entry replacement ===");
    stop_service(&fungi_bin, &node_a, &format!("{REMOTE_SERVICE}@b"))?;
    apply_service(
        &fungi_bin,
        &node_a,
        &format!("{REMOTE_SERVICE}@b"),
        &local_v1,
    )?;
    assert_service(&fungi_bin, &node_b, REMOTE_SERVICE, false, &["http"])?;
    start_service(&fungi_bin, &node_a, &format!("{REMOTE_SERVICE}@b"))?;
    apply_service(
        &fungi_bin,
        &node_a,
        &format!("{REMOTE_SERVICE}@b"),
        &local_v2_web,
    )?;
    assert_service(&fungi_bin, &node_b, REMOTE_SERVICE, true, &["web"])?;
    apply_service(
        &fungi_bin,
        &node_a,
        &format!("{REMOTE_SERVICE}@b"),
        &local_v2,
    )?;
    assert_service(&fungi_bin, &node_b, REMOTE_SERVICE, true, &["http"])?;

    wait_for_log(&fungi_bin, &node_a, LOCAL_SERVICE, "fixture-v2")?;
    wait_for_log(
        &fungi_bin,
        &node_a,
        &format!("{REMOTE_SERVICE}@b"),
        "fixture-v2",
    )?;

    println!("\n=== External host TCP access and ownership ===");
    let tcp_manifest = manifests_dir.join("host-tcp.yaml");
    fs::write(
        &tcp_manifest,
        format!(
            "fungi: service/v1\nid: host-http\npublish:\n  web:\n    tcp:\n      port: {}\n    client:\n      kind: web\n      path: /host\n",
            fixture_server.port
        ),
    )?;
    let remote_tcp = format!("{TCP_SERVICE}@b");
    apply_service(&fungi_bin, &node_a, &remote_tcp, &tcp_manifest)?;
    start_service(&fungi_bin, &node_a, &remote_tcp)?;
    let forwarded_port = TcpListener::bind(("127.0.0.1", 0))?.local_addr()?.port();
    run_cli(
        &fungi_bin,
        &node_a,
        [
            "service",
            "connect",
            &remote_tcp,
            "web",
            "--local-port",
            &forwarded_port.to_string(),
        ],
    )?;
    assert_http_response(forwarded_port, "/host", "external-host")?;
    stop_service(&fungi_bin, &node_a, &remote_tcp)?;
    assert_http_response(fixture_server.port, "/host", "external-host")?;
    start_service(&fungi_bin, &node_a, &remote_tcp)?;
    run_cli(
        &fungi_bin,
        &node_a,
        ["service", "remove", &remote_tcp, "--yes"],
    )?;
    assert_http_response(fixture_server.port, "/host", "external-host")?;

    println!("\nAll WASI apply and external TCP lab checks passed.");
    Ok(())
}

struct CleanupGuard {
    fungi_bin: PathBuf,
    fungi_lab_bin: PathBuf,
    lab_dir: PathBuf,
}

impl Drop for CleanupGuard {
    fn drop(&mut self) {
        let _ = run_cli(
            &self.fungi_bin,
            &self.lab_dir.join("nodes/a/fungi"),
            ["service", "remove", LOCAL_SERVICE, "--yes"],
        );
        let _ = run_cli(
            &self.fungi_bin,
            &self.lab_dir.join("nodes/a/fungi"),
            ["service", "remove", &format!("{REMOTE_SERVICE}@b"), "--yes"],
        );
        let _ = run_cli(
            &self.fungi_bin,
            &self.lab_dir.join("nodes/a/fungi"),
            ["service", "remove", &format!("{TCP_SERVICE}@b"), "--yes"],
        );
        let _ = run_lab(&self.fungi_lab_bin, &self.lab_dir, ["stop"]);
        if let Err(error) = run_lab(&self.fungi_lab_bin, &self.lab_dir, ["clean"]) {
            eprintln!(
                "lab cleanup failed; retained {}: {error:#}",
                self.lab_dir.display()
            );
        }
    }
}

fn workspace_root() -> Result<PathBuf> {
    let current = std::env::current_dir()?;
    for path in current.ancestors() {
        if path.join("crates").is_dir()
            && path.join("fungi").is_dir()
            && path.join("Cargo.toml").exists()
        {
            return Ok(path.to_path_buf());
        }
    }
    bail!("failed to locate fungi workspace root")
}

fn sibling_binary(name: &str) -> Result<PathBuf> {
    let current_exe = std::env::current_exe().context("failed to locate current executable")?;
    let target_dir = current_exe
        .parent()
        .context("failed to locate executable directory")?;
    let path = target_dir.join(name);
    if !path.exists() {
        bail!("required binary not found at {}", path.display());
    }
    Ok(path)
}

fn unique_suffix() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string()
}

fn write_manifest(
    dir: &Path,
    file_name: &str,
    service_name: &str,
    entry_name: &str,
    component_url: &str,
    workspace_path: &str,
) -> Result<PathBuf> {
    let path = dir.join(format!("{}-{}", unique_suffix(), file_name));
    let content = format!(
        "fungi: service/v1\nid: {service_name}\nrun:\n  provider: wasmtime\n  source:\n    url: {component_url}\n  mounts:\n    - from: $fungi.workspace\n      to: {workspace_path}\npublish:\n  {entry_name}:\n    tcp:\n      port: 8080\n    client:\n      kind: web\n      path: /\n",
    );
    fs::write(&path, content).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(path)
}

fn create_lab_dir(repo: &Path) -> Result<PathBuf> {
    let target = repo.join("target");
    fs::create_dir_all(&target)?;
    // Allocate once, atomically, even when a previous run left its data behind.
    // Only fungi-lab clean may delete it: TempDir drop must not erase state if
    // stopping the recorded processes fails.
    Ok(tempfile::Builder::new()
        .prefix("service-apply-lab-")
        .tempdir_in(target)?
        .keep())
}

#[test]
fn lab_directories_are_unique_and_preserve_previous_runs() {
    let repo = tempfile::tempdir().unwrap();
    let first = create_lab_dir(repo.path()).unwrap();
    let state = first.join("state.json");
    fs::write(&state, "previous run").unwrap();
    let second = create_lab_dir(repo.path()).unwrap();
    assert_ne!(first, second);
    assert_eq!(first.parent(), second.parent());
    assert_eq!(fs::read_to_string(state).unwrap(), "previous run");
    assert!(fs::read_dir(second).unwrap().next().is_none());
}

fn run_lab<I, S>(fungi_lab_bin: &Path, lab_dir: &Path, args: I) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let output = Command::new(fungi_lab_bin)
        .arg("--lab-dir")
        .arg(lab_dir)
        .args(
            args.into_iter()
                .map(|value| value.as_ref().to_string())
                .collect::<Vec<_>>(),
        )
        .output()
        .context("failed to execute fungi-lab command")?;
    if !output.status.success() {
        bail!(
            "fungi-lab command failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !stdout.is_empty() {
        println!("{stdout}");
    }
    Ok(stdout)
}

fn run_cli<I, S>(fungi_bin: &Path, fungi_dir: &Path, args: I) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let arg_list = args
        .into_iter()
        .map(|value| value.as_ref().to_string())
        .collect::<Vec<_>>();
    let output = Command::new(fungi_bin)
        .arg("--fungi-dir")
        .arg(fungi_dir)
        .args(&arg_list)
        .output()
        .with_context(|| format!("failed to run fungi command {:?}", arg_list))?;
    if !output.status.success() {
        bail!(
            "fungi command {:?} failed\nstdout:\n{}\nstderr:\n{}",
            arg_list,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !stdout.is_empty() {
        println!("{stdout}");
    }
    Ok(stdout)
}

fn apply_service(fungi_bin: &Path, fungi_dir: &Path, target: &str, manifest: &Path) -> Result<()> {
    let (name, device) = target
        .split_once('@')
        .map(|(name, device)| (name, Some(device)))
        .unwrap_or((target, None));
    let manifest = manifest
        .to_str()
        .context("manifest path is not valid utf-8")?;
    let mut args = vec!["service"];
    if let Some(device) = device {
        args.push("--device");
        args.push(device);
    }
    args.extend(["apply", name, "--yes"]);
    args.push(manifest);
    let output = run_cli(fungi_bin, fungi_dir, args)?;
    if !output.contains("Remote service applied:") && !output.contains("Service applied:") {
        bail!("unexpected apply output:\n{output}");
    }
    Ok(())
}

fn start_service(fungi_bin: &Path, fungi_dir: &Path, target: &str) -> Result<()> {
    let output = run_cli(fungi_bin, fungi_dir, ["service", "start", target])?;
    if !output.contains("Service started") && !output.contains("Remote service started:") {
        bail!("unexpected start output:\n{output}");
    }
    Ok(())
}

fn stop_service(fungi_bin: &Path, fungi_dir: &Path, target: &str) -> Result<()> {
    let output = run_cli(fungi_bin, fungi_dir, ["service", "stop", target])?;
    if !output.contains("Service stopped") && !output.contains("Remote service stopped:") {
        bail!("unexpected stop output:\n{output}");
    }
    Ok(())
}

fn assert_service(
    fungi_bin: &Path,
    fungi_dir: &Path,
    service: &str,
    expected_running: bool,
    expected_entries: &[&str],
) -> Result<()> {
    let inspect = run_cli(fungi_bin, fungi_dir, ["service", "inspect", service])?;
    let value: Value = serde_json::from_str(&inspect)
        .with_context(|| format!("failed to parse inspect output: {inspect}"))?;

    let phase = value
        .get("phase")
        .and_then(Value::as_str)
        .context("inspect output missing phase")?;
    let running = phase == "running";
    if running != expected_running {
        bail!(
            "service {} running mismatch: expected {}, got {}\n{}",
            service,
            expected_running,
            running,
            inspect
        );
    }

    let entries = value
        .get("entries")
        .and_then(Value::as_array)
        .context("inspect output missing entries")?
        .iter()
        .filter_map(|entry| entry.get("name").and_then(Value::as_str))
        .collect::<Vec<_>>();

    if entries != expected_entries {
        bail!(
            "service {} entries mismatch: expected {:?}, got {:?}\n{}",
            service,
            expected_entries,
            entries,
            inspect
        );
    }

    let published_entries = value
        .get("published_entries")
        .and_then(Value::as_array)
        .context("inspect output missing published_entries")?
        .iter()
        .filter_map(|entry| entry.get("name").and_then(Value::as_str))
        .collect::<Vec<_>>();

    if published_entries != expected_entries {
        bail!(
            "service {} published entries mismatch: expected {:?}, got {:?}\n{}",
            service,
            expected_entries,
            published_entries,
            inspect
        );
    }

    Ok(())
}

fn wait_for_log(fungi_bin: &Path, fungi_dir: &Path, target: &str, marker: &str) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let result = Command::new(fungi_bin)
            .arg("--fungi-dir")
            .arg(fungi_dir)
            .args(["service", "logs", target, "--tail", "20"])
            .output()?;
        if !result.status.success() {
            bail!(
                "failed to read {target} logs: {}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
        let output = String::from_utf8_lossy(&result.stdout);
        if output.contains(marker) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            bail!("{target} never logged {marker}; last logs:\n{output}");
        }
        thread::sleep(Duration::from_millis(100));
    }
}

struct FixtureServer {
    port: u16,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

impl FixtureServer {
    fn start(v1: Vec<u8>, v2: Vec<u8>) -> Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let port = listener.local_addr()?.port();
        listener.set_nonblocking(true)?;
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let worker = thread::spawn(move || {
            while !stopped.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        if let Err(error) = serve_fixture(&mut stream, &v1, &v2) {
                            eprintln!("fixture HTTP request failed: {error:#}");
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10))
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Self {
            port,
            stop,
            worker: Some(worker),
        })
    }
    fn url(&self, name: &str) -> String {
        format!("http://127.0.0.1:{}/{name}", self.port)
    }
}

impl Drop for FixtureServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn serve_fixture(stream: &mut TcpStream, v1: &[u8], v2: &[u8]) -> Result<()> {
    // Accepted sockets can inherit the listener's nonblocking mode on macOS.
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
    stream.set_write_timeout(Some(Duration::from_secs(3)))?;
    let mut reader = BufReader::new(&mut *stream);
    let mut request = String::new();
    reader.read_line(&mut request)?;
    let mut header = String::new();
    loop {
        header.clear();
        if reader.read_line(&mut header)? == 0 || header == "\r\n" {
            break;
        }
    }
    let body = if request.starts_with("GET /v1.wasm ") {
        v1
    } else if request.starts_with("GET /v2.wasm ") {
        v2
    } else {
        b"external-host\n".as_slice()
    };
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)?;
    Ok(())
}

fn assert_http_response(port: u16, path: &str, expected: &str) -> Result<()> {
    let mut stream = TcpStream::connect_timeout(
        &format!("127.0.0.1:{port}").parse()?,
        Duration::from_secs(10),
    )?;
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    stream.set_write_timeout(Some(Duration::from_secs(10)))?;
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    )?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    if !response.starts_with("HTTP/1.1 200") || !response.contains(expected) {
        bail!("unexpected response through port {port}: {response}");
    }
    Ok(())
}

fn compile_wasi_fixture(repo: &Path, output_dir: &Path, version: &str) -> Result<Vec<u8>> {
    let artifact = output_dir.join(format!("fixture-{version}.wasm"));
    let output = Command::new("rustc")
        .args(["--target", "wasm32-wasip2", "--edition", "2024", "-O"])
        .arg(repo.join("crates/tests/fixtures/long-running-wasi.rs"))
        .arg("-o")
        .arg(&artifact)
        .env("FUNGI_FIXTURE_VERSION", version)
        .output()
        .context("failed to compile WASIp2 fixture with rustc")?;
    if !output.status.success() {
        bail!(
            "WASIp2 fixture compilation failed; install the target with `rustup target add wasm32-wasip2`:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    fs::read(artifact).context("failed to read compiled WASIp2 fixture")
}
