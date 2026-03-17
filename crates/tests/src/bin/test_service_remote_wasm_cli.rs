use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::fs::File;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

const SERVICE_NAME: &str = "filebrowser-lite-wasi";
const SERVICE_ID: &str = "filebrowser-lite-wasi";
const SERVICE_PORT: u16 = 8082;
const DEFAULT_WASM_URL: &str =
    "https://github.com/enbop/filebrowser-lite/releases/latest/download/filebrowser-lite-wasi.wasm";

fn main() -> Result<()> {
    env_logger::init();

    let workspace_root = workspace_root()?;
    let fungi_bin = get_fungi_binary_path()?;
    let wasm_url = std::env::var("FUNGI_WASM_URL").unwrap_or_else(|_| DEFAULT_WASM_URL.to_string());
    let expected_text =
        std::env::var("FUNGI_WASM_EXPECT_TEXT").unwrap_or_else(|_| "filebrowser".to_string());

    println!("Using fungi binary: {}", fungi_bin.display());
    println!("Using wasm url: {wasm_url}");

    let provider_service_host_port = find_free_port()?;
    let provider = TestNode::start(&fungi_bin, "provider", provider_service_host_port)?;
    let controller = TestNode::start(&fungi_bin, "controller", find_free_port()?)?;

    let provider_peer_id = provider.info_id()?;
    let controller_peer_id = controller.info_id()?;
    println!("Provider peer id: {provider_peer_id}");
    println!("Controller peer id: {controller_peer_id}");

    provider.allowed_peer_add(&controller_peer_id)?;
    wait_for_mdns_peer(&provider, &controller_peer_id, Duration::from_secs(45))?;
    wait_for_mdns_peer(&controller, &provider_peer_id, Duration::from_secs(45))?;

    let manifest_path = write_manifest_file(controller.temp_dir().path(), &wasm_url)?;

    run_local_service_checks(
        &provider,
        &manifest_path,
        provider_service_host_port,
        &expected_text,
    )?;
    run_remote_service_checks(
        &provider,
        &controller,
        &provider_peer_id,
        &manifest_path,
        provider_service_host_port,
        &expected_text,
    )?;

    println!("\nAll WASM local + remote service smoke checks passed.");
    println!("Provider log: {}", provider.log_file().display());
    println!("Controller log: {}", controller.log_file().display());
    println!("Workspace root: {}", workspace_root.display());
    Ok(())
}

struct TestNode {
    name: String,
    temp_dir: TempDir,
    fungi_dir: PathBuf,
    log_file: PathBuf,
    child: Child,
}

impl TestNode {
    fn start(fungi_bin: &Path, name: &str, allowed_service_port: u16) -> Result<Self> {
        let temp_dir = TempDir::new().context("failed to create temp dir")?;
        let fungi_dir = temp_dir.path().join("fungi-home");
        let log_file = temp_dir.path().join(format!("{name}-daemon.log"));
        std::fs::create_dir_all(&fungi_dir).context("failed to create fungi dir")?;

        run_command(
            Command::new(fungi_bin)
                .arg("--fungi-dir")
                .arg(&fungi_dir)
                .arg("init"),
        )
        .with_context(|| format!("failed to init node {name}"))?;

        let rpc_port = find_free_port()?;
        let listen_tcp_port = find_free_port()?;
        let listen_udp_port = find_free_port()?;
        let config_toml = format!(
            "[rpc]\nlisten_address = \"127.0.0.1:{rpc_port}\"\n\n[network]\nlisten_tcp_port = {listen_tcp_port}\nlisten_udp_port = {listen_udp_port}\ndisable_relay = true\n\n[runtime]\ndisable_docker = true\ndisable_wasmtime = false\nallowed_host_paths = [\"{}\", \"{}\"]\nallowed_ports = [{allowed_service_port}]\nallowed_port_ranges = []\n",
            fungi_dir.display(),
            fungi_dir.join("services").display(),
        );
        std::fs::write(fungi_dir.join("config.toml"), config_toml)
            .with_context(|| format!("failed to write config for node {name}"))?;

        let stdout = File::create(&log_file).context("failed to create log file")?;
        let stderr = stdout.try_clone().context("failed to clone log file")?;
        let child = Command::new(fungi_bin)
            .arg("--fungi-dir")
            .arg(&fungi_dir)
            .arg("daemon")
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr))
            .spawn()
            .with_context(|| format!("failed to start daemon for node {name}"))?;

        let node = Self {
            name: name.to_string(),
            temp_dir,
            fungi_dir,
            log_file,
            child,
        };
        node.wait_ready(Duration::from_secs(30))?;
        Ok(node)
    }

    fn temp_dir(&self) -> &TempDir {
        &self.temp_dir
    }

    fn log_file(&self) -> &Path {
        &self.log_file
    }

    fn wait_ready(&self, timeout: Duration) -> Result<()> {
        let started = Instant::now();
        while started.elapsed() < timeout {
            if self.run_cli(["info", "version"]).is_ok() {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(500));
        }

        bail!(
            "daemon {} did not become ready in {:?}\n{}",
            self.name,
            timeout,
            self.tail_log(80),
        )
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

        let output = Command::new(fungi_bin)
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
                self.tail_log(80),
            );
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn info_id(&self) -> Result<String> {
        self.run_cli(["info", "id"])
    }

    fn allowed_peer_add(&self, peer_id: &str) -> Result<()> {
        let output = self.run_cli(["allowed-peers", "add", peer_id])?;
        if !output.contains("Peer added successfully") {
            bail!("unexpected allowlist output on {}: {output}", self.name);
        }
        Ok(())
    }

    fn tail_log(&self, lines: usize) -> String {
        let Ok(contents) = std::fs::read_to_string(&self.log_file) else {
            return format!("<failed to read log {}>", self.log_file.display());
        };
        let all_lines = contents.lines().collect::<Vec<_>>();
        let start = all_lines.len().saturating_sub(lines);
        format!(
            "== {} daemon log tail ==\n{}",
            self.name,
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

fn run_local_service_checks(
    provider: &TestNode,
    manifest_path: &Path,
    host_port: u16,
    expected_text: &str,
) -> Result<()> {
    println!("\n=== Local service smoke ===");

    let pull_output = provider.run_cli([
        "service",
        "pull",
        manifest_path
            .to_str()
            .context("manifest path is not valid utf-8")?,
    ])?;
    assert_contains(&pull_output, SERVICE_NAME, "local pull output")?;

    let list_output = provider.run_cli(["service", "list"])?;
    assert_contains(&list_output, SERVICE_NAME, "local service list")?;

    let inspect_before = parse_json(&provider.run_cli(["service", "inspect", SERVICE_NAME])?)?;
    assert_json_string(
        &inspect_before,
        "name",
        SERVICE_NAME,
        "local inspect before start",
    )?;
    assert_json_bool(
        &inspect_before,
        "running",
        false,
        "local inspect before start",
    )?;

    let start_output = provider.run_cli(["service", "start", SERVICE_NAME])?;
    assert_contains(&start_output, "Service started", "local start output")?;

    wait_for_http_ok(host_port, expected_text, Duration::from_secs(90))?;

    let inspect_running = parse_json(&provider.run_cli(["service", "inspect", SERVICE_NAME])?)?;
    assert_json_bool(
        &inspect_running,
        "running",
        true,
        "local inspect after start",
    )?;
    assert_contains(
        &provider.run_cli(["service", "logs", SERVICE_NAME, "--tail", "50"])?,
        "",
        "local logs output",
    )?;

    let stop_output = provider.run_cli(["service", "stop", SERVICE_NAME])?;
    assert_contains(&stop_output, "Service stopped", "local stop output")?;

    let inspect_stopped = parse_json(&provider.run_cli(["service", "inspect", SERVICE_NAME])?)?;
    assert_json_bool(
        &inspect_stopped,
        "running",
        false,
        "local inspect after stop",
    )?;

    let remove_output = provider.run_cli(["service", "remove", SERVICE_NAME])?;
    assert_contains(&remove_output, "Service removed", "local remove output")?;

    let list_after_remove = provider.run_cli(["service", "list"])?;
    if list_after_remove.contains(SERVICE_NAME) {
        bail!("service still present after local remove\n{list_after_remove}");
    }

    Ok(())
}

fn run_remote_service_checks(
    provider: &TestNode,
    controller: &TestNode,
    provider_peer_id: &str,
    manifest_path: &Path,
    provider_host_port: u16,
    expected_text: &str,
) -> Result<()> {
    println!("\n=== Remote service smoke ===");

    let capabilities_output =
        controller.run_cli(["peer", "capability", "--peer", provider_peer_id])?;
    assert_contains(
        &capabilities_output,
        "\"wasmtime\": true",
        "peer capability summary",
    )?;

    let remote_pull_output = controller.run_cli([
        "remote",
        "service",
        "pull",
        provider_peer_id,
        manifest_path
            .to_str()
            .context("manifest path is not valid utf-8")?,
    ])?;
    assert_contains(&remote_pull_output, SERVICE_NAME, "remote pull output")?;

    let remote_list_output = controller.run_cli(["remote", "service", "list", provider_peer_id])?;
    assert_contains(&remote_list_output, SERVICE_NAME, "remote list output")?;

    let remote_start_output =
        controller.run_cli(["remote", "service", "start", provider_peer_id, SERVICE_NAME])?;
    assert_contains(&remote_start_output, SERVICE_NAME, "remote start output")?;

    wait_for_http_ok(provider_host_port, expected_text, Duration::from_secs(90))?;

    let remote_discover_output =
        controller.run_cli(["remote", "service", "discover", provider_peer_id])?;
    assert_contains(
        &remote_discover_output,
        SERVICE_ID,
        "remote discover output",
    )?;

    let remote_forward_output =
        controller.run_cli(["remote", "service", "forward", provider_peer_id, SERVICE_ID])?;
    let forwarded = parse_json(&remote_forward_output)?;
    let forwarded_port = forwarded
        .get("endpoints")
        .and_then(Value::as_array)
        .and_then(|endpoints| endpoints.first())
        .and_then(|entry| entry.get("local_port"))
        .and_then(Value::as_u64)
        .context("missing forwarded local_port")? as u16;
    wait_for_http_ok(forwarded_port, expected_text, Duration::from_secs(45))?;

    let forwarded_list_output =
        controller.run_cli(["remote", "service", "forwarded", provider_peer_id])?;
    assert_contains(
        &forwarded_list_output,
        SERVICE_NAME,
        "remote forwarded list",
    )?;

    let remote_unforward_output = controller.run_cli([
        "remote",
        "service",
        "unforward",
        provider_peer_id,
        SERVICE_ID,
    ])?;
    assert_contains(
        &remote_unforward_output,
        "Remote service local forwarding removed",
        "remote unforward output",
    )?;

    let forwarded_after_unforward =
        controller.run_cli(["remote", "service", "forwarded", provider_peer_id])?;
    if forwarded_after_unforward.contains(SERVICE_NAME) {
        bail!("remote forwarding still present after unforward\n{forwarded_after_unforward}");
    }

    let remote_stop_output =
        controller.run_cli(["remote", "service", "stop", provider_peer_id, SERVICE_NAME])?;
    assert_contains(&remote_stop_output, SERVICE_NAME, "remote stop output")?;
    let inspect_stopped = parse_json(&provider.run_cli(["service", "inspect", SERVICE_NAME])?)?;
    assert_json_bool(
        &inspect_stopped,
        "running",
        false,
        "provider inspect after remote stop",
    )?;

    let remote_remove_output = controller.run_cli([
        "remote",
        "service",
        "remove",
        provider_peer_id,
        SERVICE_NAME,
    ])?;
    assert_contains(&remote_remove_output, SERVICE_NAME, "remote remove output")?;
    let provider_list_after_remove = provider.run_cli(["service", "list"])?;
    if provider_list_after_remove.contains(SERVICE_NAME) {
        bail!("provider still lists service after remote remove\n{provider_list_after_remove}");
    }

    Ok(())
}

fn wait_for_mdns_peer(node: &TestNode, peer_id: &str, timeout: Duration) -> Result<()> {
    let started = Instant::now();
    while started.elapsed() < timeout {
        let output = node.run_cli(["device", "mdns"])?;
        if output.contains(peer_id) {
            return Ok(());
        }
        thread::sleep(Duration::from_secs(1));
    }

    bail!(
        "node {} did not discover peer {} via mDNS\n{}",
        node.name,
        peer_id,
        node.tail_log(120),
    )
}

fn wait_for_http_ok(port: u16, expected_text: &str, timeout: Duration) -> Result<()> {
    let started = Instant::now();
    let expected_lower = expected_text.to_lowercase();
    while started.elapsed() < timeout {
        if let Ok(response) = http_get(port) {
            let response_lower = response.to_lowercase();
            if response.contains("200 OK")
                && (expected_lower.is_empty() || response_lower.contains(&expected_lower))
            {
                return Ok(());
            }
        }
        thread::sleep(Duration::from_secs(1));
    }

    bail!(
        "http endpoint on 127.0.0.1:{port} did not become healthy within {:?}",
        timeout
    )
}

fn http_get(port: u16) -> Result<String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    Ok(response)
}

fn write_manifest_file(dir: &Path, wasm_url: &str) -> Result<PathBuf> {
    let manifest_path = dir.join("filebrowser-lite-wasi.service.yaml");
    let manifest_yaml = format!(
        "apiVersion: fungi.dev/v1alpha1\nkind: ServiceManifest\n\nmetadata:\n  name: {service_name}\n  labels:\n    app: {service_name}\n    managedBy: fungi\n\nspec:\n  runtime: wasmtime\n\n  expose:\n    enabled: true\n    serviceId: {service_id}\n    displayName: File Browser Lite\n    transport:\n      kind: tcp\n    usage:\n      kind: web\n      path: /\n    iconUrl: https://raw.githubusercontent.com/filebrowser/logo/master/icon.svg\n    catalogId: io.enbop.filebrowser-lite-wasi\n\n  source:\n    url: {wasm_url}\n\n  ports:\n    - hostPort: auto\n      name: http\n      servicePort: {service_port}\n      protocol: tcp\n\n  mounts:\n    - hostPath: ${{APP_HOME}}/data\n      runtimePath: data\n\n  env: {{}}\n\n  command: []\n  entrypoint: []\n\n  workingDir: null\n",
        service_name = SERVICE_NAME,
        service_id = SERVICE_ID,
        service_port = SERVICE_PORT,
    );
    std::fs::write(&manifest_path, manifest_yaml).context("failed to write manifest file")?;
    Ok(manifest_path)
}

fn parse_json(raw: &str) -> Result<Value> {
    serde_json::from_str(raw).with_context(|| format!("failed to parse json output:\n{raw}"))
}

fn assert_contains(haystack: &str, needle: &str, label: &str) -> Result<()> {
    if needle.is_empty() || haystack.contains(needle) {
        return Ok(());
    }
    bail!("{label} did not contain {needle:?}\n{haystack}")
}

fn assert_json_string(value: &Value, key: &str, expected: &str, label: &str) -> Result<()> {
    let actual = value
        .get(key)
        .and_then(Value::as_str)
        .with_context(|| format!("{label} missing string field {key}"))?;
    if actual == expected {
        return Ok(());
    }
    bail!("{label} expected {key}={expected:?}, got {actual:?}")
}

fn assert_json_bool(value: &Value, key: &str, expected: bool, label: &str) -> Result<()> {
    let actual = value
        .get(key)
        .and_then(Value::as_bool)
        .with_context(|| format!("{label} missing bool field {key}"))?;
    if actual == expected {
        return Ok(());
    }
    bail!("{label} expected {key}={expected}, got {actual}")
}

fn find_free_port() -> Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).context("failed to bind ephemeral port")?;
    Ok(listener.local_addr()?.port())
}

fn workspace_root() -> Result<PathBuf> {
    let current_exe = std::env::current_exe().context("failed to get current exe")?;
    let target_dir = current_exe.parent().context("failed to get target dir")?;
    target_dir
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .context("failed to derive workspace root")
}

fn get_fungi_binary_path() -> Result<PathBuf> {
    let current_exe = std::env::current_exe().context("failed to get current exe")?;
    let target_dir = current_exe.parent().context("failed to get target dir")?;
    let fungi_bin = target_dir.join("fungi");
    if !fungi_bin.exists() {
        bail!(
            "fungi binary not found at {}. Build it first with: cargo build --bin fungi",
            fungi_bin.display()
        );
    }
    Ok(fungi_bin)
}

fn run_command(command: &mut Command) -> Result<Output> {
    let output = command.output().context("failed to execute command")?;
    if output.status.success() {
        return Ok(output);
    }

    bail!(
        "command failed with status {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    )
}
