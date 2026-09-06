use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use fungi_config::{
    FungiConfig,
    devices::{DeviceInfo, DevicesConfig},
    local_preferences::{LocalPortSource, LocalPreferenceCache, LocalServicePreference},
};
use tempfile::TempDir;

struct DaemonChild {
    child: Child,
}

impl Drop for DaemonChild {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl DaemonChild {
    fn stop_gracefully(mut self) {
        drop(self.child.stdin.take());
        let status = self.child.wait().unwrap();
        assert!(status.success(), "daemon exited with {status}");
    }

    fn kill_and_wait(&mut self) {
        self.child.kill().unwrap();
        self.child.wait().unwrap();
    }
}

#[test]
fn daemon_publishes_dynamic_endpoint_and_enforces_one_instance_per_fungi_dir() {
    let home = TempDir::new().unwrap();
    let swarm = reserve_port();
    init_fungi_dir(home.path(), 0, swarm);

    let daemon = start_daemon(home.path());
    let _peer = wait_peer_id(home.path());
    let endpoint = fungi_config::read_daemon_endpoint(home.path()).unwrap();
    let address = endpoint.strip_prefix("http://").unwrap();
    let address: std::net::SocketAddr = address.parse().unwrap();
    assert_ne!(address.port(), 0);

    let output = run_cli(home.path(), ["info", "rpc-address"]);
    assert_eq!(output.stdout.trim(), address.to_string());

    let duplicate = run_cli_result(home.path(), ["daemon", "--exit-on-stdin-close"], "");
    assert!(!duplicate.status.success());
    assert!(
        duplicate.stdout.contains("already running")
            || duplicate.stderr.contains("already running"),
        "stdout:\n{}\nstderr:\n{}",
        duplicate.stdout,
        duplicate.stderr
    );
    assert_eq!(
        fungi_config::read_daemon_endpoint(home.path()).unwrap(),
        endpoint
    );

    daemon.stop_gracefully();
    assert!(!fungi_config::daemon_endpoint_path(home.path()).exists());
    assert!(fungi_config::daemon_lock_path(home.path()).exists());

    let mut restarted = start_daemon(home.path());
    let _peer = wait_peer_id(home.path());
    restarted.kill_and_wait();
    assert!(fungi_config::daemon_endpoint_path(home.path()).exists());

    let recovered = start_daemon(home.path());
    let _peer = wait_peer_id(home.path());
    let recovered_endpoint = fungi_config::read_daemon_endpoint(home.path()).unwrap();
    assert!(recovered_endpoint.starts_with("http://127.0.0.1:"));
    recovered.stop_gracefully();
}

#[test]
fn daemon_publishes_endpoint_before_saved_service_access_refresh_finishes() {
    let home = TempDir::new().unwrap();
    let swarm = reserve_port();
    init_fungi_dir(home.path(), 0, swarm);

    let blackhole = TcpListener::bind("127.0.0.1:0").unwrap();
    let blackhole_port = blackhole.local_addr().unwrap().port();
    thread::spawn(move || {
        if let Ok((_stream, _)) = blackhole.accept() {
            thread::sleep(Duration::from_secs(20));
        }
    });

    let peer_id = libp2p::PeerId::random();
    let mut device = DeviceInfo::new_unknown(peer_id);
    device.name = Some("slow-device".to_string());
    device.multiaddrs = vec![format!("/ip4/127.0.0.1/tcp/{blackhole_port}/p2p/{peer_id}")];
    DevicesConfig::apply_from_dir(home.path())
        .unwrap()
        .add_or_update_device(device)
        .unwrap();
    LocalPreferenceCache::apply_from_dir(home.path())
        .unwrap()
        .upsert_record(LocalServicePreference {
            remote_peer_id: peer_id.to_string(),
            remote_service_name: "slow-service".to_string(),
            remote_service_port_name: "main".to_string(),
            local_host: "127.0.0.1".to_string(),
            local_port: reserve_port(),
            local_port_source: LocalPortSource::Auto,
        })
        .unwrap();

    let daemon = start_daemon(home.path());
    let started = Instant::now();
    let deadline = started + Duration::from_secs(3);
    while !fungi_config::daemon_endpoint_path(home.path()).exists() {
        assert!(
            Instant::now() < deadline,
            "daemon endpoint publication waited for background service access refresh"
        );
        thread::sleep(Duration::from_millis(25));
    }

    let output = run_cli(home.path(), ["info", "version"]);
    assert!(!output.stdout.trim().is_empty());
    assert!(started.elapsed() < Duration::from_secs(3));
    daemon.stop_gracefully();
}

#[test]
fn service_apply_file_without_target_prints_order_hint() {
    let home = TempDir::new().unwrap();
    let manifest = home.path().join("demo.fungi.md");
    let manifest_arg = manifest.to_string_lossy().to_string();

    let output = run_cli_result(home.path(), ["service", "apply", manifest_arg.as_str()], "");

    assert!(!output.status.success());
    assert_eq!(output.stdout, "");
    assert!(
        output
            .stderr
            .contains("missing NAME[@DEVICE] before the file"),
        "{}",
        output.stderr
    );
    assert!(
        output
            .stderr
            .contains("fungi service apply <name[@device]> <file>"),
        "{}",
        output.stderr
    );
}

#[test]
fn service_apply_dry_run_prints_resolved_intent() {
    let home = TempDir::new().unwrap();
    let rpc = reserve_port();
    let swarm = reserve_port();

    init_fungi_dir(home.path(), rpc, swarm);
    let _daemon = start_daemon(home.path());
    let _peer = wait_peer_id(home.path());
    let manifest = write_wasmtime_dry_run_manifest(home.path());
    let manifest_path = manifest.to_string_lossy();

    let output = run_cli_result(
        home.path(),
        [
            "service",
            "apply",
            "dry-run-wasi",
            "--dry-run",
            "--start",
            manifest_path.as_ref(),
        ],
        "",
    );

    assert!(
        output.status.success(),
        "dry-run failed\nstdout:\n{}\nstderr:\n{}",
        output.stdout,
        output.stderr
    );
    assert_eq!(output.stderr, "");
    assert!(output.stdout.contains("Service: dry-run-wasi"));
    assert!(output.stdout.contains("Run:\n  runtime: wasmtime"));
    assert!(output.stdout.contains("  invocation: run"));
    assert!(output.stdout.contains("Mounts:"));
    assert!(output.stdout.contains(" -> /"));
    assert!(
        output
            .stdout
            .contains("Publish:\n  main: tcp service:8080 daemon:8080 (fixed)")
    );
    assert!(output.stdout.contains("Runtime grants:"));
    assert!(output.stdout.contains("  - tcp"));
    assert!(output.stdout.contains("Warnings:"));
    assert!(
        output
            .stdout
            .contains("$fungi.root exposes the full Fungi user root")
    );
    assert!(
        output
            .stdout
            .contains("After apply: ensure service is running")
    );
}

#[test]
fn service_apply_start_reports_and_preserves_the_final_running_state() {
    let home = TempDir::new().unwrap();
    let rpc = reserve_port();
    let swarm = reserve_port();
    let target = reserve_port();

    init_fungi_dir(home.path(), rpc, swarm);
    let _daemon = start_daemon(home.path());
    let _peer = wait_peer_id(home.path());
    let manifest = write_existing_tcp_service_manifest(home.path(), "apply-start", target, "raw");
    let manifest_path = manifest.to_string_lossy();

    let first = run_cli_result(
        home.path(),
        [
            "service",
            "apply",
            "apply-start",
            "--yes",
            "--start",
            manifest_path.as_ref(),
        ],
        "",
    );
    assert!(
        first.status.success(),
        "first apply failed\nstdout:\n{}\nstderr:\n{}",
        first.stdout,
        first.stderr
    );
    assert!(first.stdout.contains("Service applied: apply-start"));
    assert!(first.stdout.contains("Manifest: created"));
    assert!(first.stdout.contains("Workload: none"));
    assert!(first.stdout.contains("Final phase: running"));

    let second = run_cli_result(
        home.path(),
        [
            "service",
            "apply",
            "apply-start",
            "--yes",
            "--start",
            manifest_path.as_ref(),
        ],
        "",
    );
    assert!(
        second.status.success(),
        "second apply failed\nstdout:\n{}\nstderr:\n{}",
        second.stdout,
        second.stderr
    );
    assert!(second.stdout.contains("Manifest: unchanged"));
    assert!(second.stdout.contains("Workload: none"));
    assert!(second.stdout.contains("Final phase: running"));
}

#[test]
fn service_apply_help_explains_state_aware_start_behavior() {
    let home = TempDir::new().unwrap();
    let output = run_cli_result(home.path(), ["service", "apply", "--help"], "");

    assert!(output.status.success(), "{}", output.stderr);
    assert!(
        output
            .stdout
            .contains("Ensure the service is running after applying it")
    );
    assert!(output.stdout.contains("First deployment"));
    assert!(output.stdout.contains("Running service update"));
    assert!(output.stdout.contains("Stopped service update"));
}

#[test]
fn service_apply_rejects_mismatched_definition_id() {
    let home = TempDir::new().unwrap();
    let rpc = reserve_port();
    let swarm = reserve_port();
    let code_server_port = reserve_port();
    let filebrowser_port = reserve_port();

    init_fungi_dir(home.path(), rpc, swarm);
    let _daemon = start_daemon(home.path());
    let _peer = wait_peer_id(home.path());

    let code_server =
        write_existing_tcp_service_manifest(home.path(), "code-server", code_server_port, "raw");
    let code_server_path = code_server.to_string_lossy();
    run_cli(
        home.path(),
        [
            "service",
            "apply",
            "code-server",
            "--yes",
            code_server_path.as_ref(),
        ],
    );

    let filebrowser = write_existing_tcp_service_manifest(
        home.path(),
        "filebrowser-lite",
        filebrowser_port,
        "raw",
    );
    let filebrowser_path = filebrowser.to_string_lossy();
    let output = run_cli_result(
        home.path(),
        [
            "service",
            "apply",
            "code-server",
            "--yes",
            filebrowser_path.as_ref(),
        ],
        "",
    );

    assert!(!output.status.success());
    assert_eq!(output.stdout, "");
    assert!(
        output.stderr.contains("definition id `code-server`"),
        "{}",
        output.stderr
    );
    assert!(
        output
            .stderr
            .contains("new manifest declares `filebrowser-lite`"),
        "{}",
        output.stderr
    );
}

#[test]
fn cli_requires_local_scope_for_existing_dynamic_service() {
    let home = TempDir::new().unwrap();
    let rpc = reserve_port();
    let swarm = reserve_port();
    let target = reserve_port();

    init_fungi_dir(home.path(), rpc, swarm);
    let _daemon = start_daemon(home.path());
    let _peer = wait_peer_id(home.path());

    let manifest = write_existing_tcp_service_manifest(home.path(), "devices", target, "raw");
    let manifest_path = manifest.to_string_lossy();
    run_cli(
        home.path(),
        [
            "service",
            "apply",
            "devices",
            "--start",
            manifest_path.as_ref(),
        ],
    );

    let output = run_cli_result(home.path(), ["devices"], "");

    assert!(!output.status.success());
    assert_eq!(output.stdout, "");
    assert!(
        output.stderr.contains("unrecognized subcommand 'devices'")
            || output.stderr.contains("unrecognized subcommand `devices`"),
        "{}",
        output.stderr
    );
    assert!(output.stderr.contains("device"), "{}", output.stderr);

    let output = run_cli_result(home.path(), ["devices@local"], "");

    assert!(!output.status.success());
    assert_eq!(output.stdout, "");
    assert_eq!(
        output.stderr,
        "No web entry is available for this service\n"
    );
}

#[test]
fn cli_rejects_open_and_connect_for_stopped_local_service() {
    let home = TempDir::new().unwrap();
    let rpc = reserve_port();
    let swarm = reserve_port();
    let target = reserve_port();

    init_fungi_dir(home.path(), rpc, swarm);
    let _daemon = start_daemon(home.path());
    let _peer = wait_peer_id(home.path());

    let manifest = write_existing_tcp_service_manifest(home.path(), "stopped-web", target, "web");
    let manifest_path = manifest.to_string_lossy();
    run_cli(
        home.path(),
        [
            "service",
            "apply",
            "stopped-web",
            "--yes",
            manifest_path.as_ref(),
        ],
    );

    let assert_rejected = |output: CliOutput| {
        assert!(
            !output.status.success(),
            "{:?} unexpectedly succeeded",
            output.args
        );
        assert_eq!(output.stdout, "");
        assert!(
            output
                .stderr
                .contains("Local service stopped-web exists but is not running (phase: stopped)"),
            "command {:?}\nstderr:\n{}",
            output.args,
            output.stderr
        );
        assert!(
            output.stderr.contains("fungi service start stopped-web"),
            "command {:?}\nstderr:\n{}",
            output.args,
            output.stderr
        );
    };

    assert_rejected(run_cli_result_with_retry(
        home.path(),
        ["service", "open", "stopped-web"],
        "",
    ));
    assert_rejected(run_cli_result_with_retry(
        home.path(),
        ["service", "connect", "stopped-web"],
        "",
    ));
    assert_rejected(run_cli_result_with_retry(
        home.path(),
        ["stopped-web@local"],
        "",
    ));
}

#[test]
fn cli_rejects_reserved_local_device_name() {
    let home = TempDir::new().unwrap();

    let output = run_cli_result(home.path(), ["device", "add", "local", "not-a-peer-id"], "");

    assert!(!output.status.success());
    assert_eq!(output.stdout, "");
    assert!(
        output
            .stderr
            .contains("Device name `local` is reserved for the local device"),
        "{}",
        output.stderr
    );
}

#[test]
fn cli_can_interactively_create_local_tcp_service() {
    let home = TempDir::new().unwrap();
    let rpc = reserve_port();
    let swarm = reserve_port();
    let target = reserve_port();

    init_fungi_dir(home.path(), rpc, swarm);
    let _daemon = start_daemon(home.path());
    let _peer = wait_peer_id(home.path());

    let target_listener = TcpListener::bind(("127.0.0.1", target)).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = target_listener.accept().unwrap();
        let mut buf = [0_u8; 4];
        stream.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"ping");
        stream.write_all(b"pong").unwrap();
    });

    let input = format!("\n{target}\n\n\ny\n");
    run_cli_with_input(
        home.path(),
        ["service", "apply", "created-raw", "--create", "--yes"],
        &input,
    );

    let output = run_cli(home.path(), ["service", "connect", "created-raw"]);
    let local_addr = output.stdout.trim();
    let mut stream = connect_with_retry(local_addr, Duration::from_secs(5));
    stream.write_all(b"ping").unwrap();
    let mut response = [0_u8; 4];
    stream.read_exact(&mut response).unwrap();
    assert_eq!(&response, b"pong");

    server.join().unwrap();
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Windows GitHub Actions intermittently cancels short-lived local gRPC CLI connections in this two-daemon smoke; Linux/macOS cover the full remote TCP service flow"
)]
fn cli_can_create_and_access_remote_tcp_service() {
    let a = TempDir::new().unwrap();
    let b = TempDir::new().unwrap();
    let a_rpc = reserve_port();
    let b_rpc = reserve_port();
    let a_swarm = reserve_port();
    let b_swarm = reserve_port();

    init_fungi_dir(a.path(), a_rpc, a_swarm);
    init_fungi_dir(b.path(), b_rpc, b_swarm);

    let _daemon_a = start_daemon(a.path());
    let _daemon_b = start_daemon(b.path());

    let a_peer = wait_peer_id(a.path());
    let b_peer = wait_peer_id(b.path());
    let b_addr = format!("/ip4/127.0.0.1/tcp/{b_swarm}/p2p/{b_peer}");

    run_cli(
        a.path(),
        [
            "device",
            "add",
            "b",
            b_peer.as_str(),
            "--addr",
            b_addr.as_str(),
        ],
    );
    run_cli(b.path(), ["device", "add", "a", a_peer.as_str()]);
    run_cli_with_input(b.path(), ["device", "trust", "a"], "y\n");

    let target = TcpListener::bind("127.0.0.1:0").unwrap();
    let target_port = target.local_addr().unwrap().port();
    let server = thread::spawn(move || {
        for _ in 0..2 {
            let (mut stream, _) = target.accept().unwrap();
            let mut buf = [0_u8; 4];
            stream.read_exact(&mut buf).unwrap();
            assert_eq!(&buf, b"ping");
            stream.write_all(b"pong").unwrap();
        }
    });

    let manifest = write_existing_tcp_service_manifest(a.path(), "test-tcp", target_port, "raw");
    let manifest_path = manifest.to_string_lossy();
    run_cli(
        a.path(),
        [
            "service",
            "--device",
            "b",
            "apply",
            "test-tcp",
            manifest_path.as_ref(),
        ],
    );
    assert!(
        a.path()
            .join("cache")
            .join("device_service_snapshots")
            .join(format!("{b_peer}.json"))
            .exists(),
        "device service snapshot cache should persist outside config.toml"
    );

    let output = run_cli_result(a.path(), ["test-tcp@b"], "");
    assert!(!output.status.success());
    assert!(
        output
            .stderr
            .contains("Remote service test-tcp@b exists but is not running"),
        "{}",
        output.stderr
    );
    assert!(
        output.stderr.contains("fungi service start test-tcp@b"),
        "{}",
        output.stderr
    );

    run_cli(a.path(), ["service", "start", "test-tcp@b"]);
    let reapplied = run_cli(
        a.path(),
        [
            "service",
            "apply",
            "test-tcp@b",
            "--yes",
            "--start",
            manifest_path.as_ref(),
        ],
    );
    assert!(
        reapplied
            .stdout
            .contains("Remote service applied: test-tcp@b")
    );
    assert!(reapplied.stdout.contains("Manifest: unchanged"));
    assert!(reapplied.stdout.contains("Workload: none"));
    assert!(reapplied.stdout.contains("Final phase: running"));

    let output = run_cli(a.path(), ["test-tcp@b"]);
    let local_addr = extract_local_address(&output.stdout);
    assert!(
        a.path()
            .join("cache")
            .join("local_preferences.json")
            .exists(),
        "local preferences should persist outside config.toml"
    );
    let access_json =
        std::fs::read_to_string(a.path().join("cache").join("local_preferences.json")).unwrap();
    assert!(access_json.contains("test-tcp"));
    assert!(access_json.trim_start().starts_with('['));
    assert!(!access_json.contains("\"rules\""));
    let config_toml = std::fs::read_to_string(a.path().join("config.toml")).unwrap();
    assert!(
        !config_toml.contains("remote_service_id"),
        "local preferences should not be persisted in config.toml"
    );

    let mut stream = connect_with_retry(&local_addr, Duration::from_secs(5));
    stream.write_all(b"ping").unwrap();
    let mut response = [0_u8; 4];
    stream.read_exact(&mut response).unwrap();
    assert_eq!(&response, b"pong");

    run_cli(a.path(), ["service", "stop", "test-tcp@b"]);
    assert!(
        TcpStream::connect(&local_addr).is_err(),
        "service stop should release the local listener"
    );
    let output = run_cli(a.path(), ["service"]);
    assert!(
        output.stdout.contains("test-tcp@b"),
        "stopped remote service should remain visible\n{}",
        output.stdout
    );
    assert!(
        output.stdout.contains("stopped"),
        "stopped remote service should keep its state\n{}",
        output.stdout
    );

    run_cli(a.path(), ["service", "start", "test-tcp@b"]);
    let mut stream = connect_with_retry(&local_addr, Duration::from_secs(5));
    stream.write_all(b"ping").unwrap();
    let mut response = [0_u8; 4];
    stream.read_exact(&mut response).unwrap();
    assert_eq!(
        &response, b"pong",
        "remote service start should restore the saved local listener"
    );
    server.join().unwrap();

    let output = run_cli(a.path(), ["test-tcp@b"]);
    let restarted_local_addr = extract_local_address(&output.stdout);
    assert_eq!(
        restarted_local_addr, local_addr,
        "remote service restart should reuse the saved local address"
    );

    run_cli(a.path(), ["service", "disconnect", "test-tcp@b"]);
    assert!(
        TcpStream::connect(&local_addr).is_err(),
        "service disconnect should release the local listener"
    );
    let output = run_cli(a.path(), ["test-tcp@b"]);
    let reconnected_local_addr = extract_local_address(&output.stdout);
    assert_eq!(
        reconnected_local_addr, local_addr,
        "service reconnect should reuse the saved local address"
    );

    run_cli(a.path(), ["device", "remove", "b"]);
    let access_json =
        std::fs::read_to_string(a.path().join("cache").join("local_preferences.json")).unwrap();
    assert!(
        !access_json.contains("test-tcp"),
        "device remove should clear local preferences"
    );
    let output = run_cli(a.path(), ["device"]);
    assert!(
        !output.stdout.contains(" - b "),
        "device remove should accept saved device names\n{}",
        output.stdout
    );
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Windows GitHub Actions intermittently cancels short-lived local gRPC CLI connections in two-daemon smoke tests"
)]
fn cli_reads_remote_service_logs_with_default_and_explicit_tail() {
    let a = TempDir::new().unwrap();
    let b = TempDir::new().unwrap();
    let a_rpc = reserve_port();
    let b_rpc = reserve_port();
    let a_swarm = reserve_port();
    let b_swarm = reserve_port();

    init_fungi_dir(a.path(), a_rpc, a_swarm);
    init_fungi_dir(b.path(), b_rpc, b_swarm);

    let _daemon_a = start_daemon(a.path());
    let _daemon_b = start_daemon(b.path());

    let a_peer = wait_peer_id(a.path());
    let b_peer = wait_peer_id(b.path());
    let b_addr = format!("/ip4/127.0.0.1/tcp/{b_swarm}/p2p/{b_peer}");

    run_cli(
        a.path(),
        [
            "device",
            "add",
            "b",
            b_peer.as_str(),
            "--addr",
            b_addr.as_str(),
        ],
    );
    run_cli(b.path(), ["device", "add", "a", a_peer.as_str()]);
    run_cli_with_input(b.path(), ["device", "trust", "a"], "y\n");

    let component = b.path().join("remote-log-demo.wasm");
    std::fs::write(&component, b"test component bytes").unwrap();
    let manifest = write_wasmtime_service_manifest(a.path(), &component, "remote-log-demo");
    let manifest_path = manifest.to_string_lossy();
    run_cli(
        a.path(),
        [
            "service",
            "apply",
            "remote-log-demo@b",
            "--yes",
            manifest_path.as_ref(),
        ],
    );

    let log_path = b
        .path()
        .join("runtime/wasmtime/remote-log-demo/runtime.log");
    let text = (0..210)
        .map(|line| format!("cli remote log {line:03}\n"))
        .collect::<String>();
    std::fs::write(log_path, text).unwrap();

    let output = run_cli(a.path(), ["service", "logs", "remote-log-demo@b"]);
    assert_eq!(output.stderr, "");
    assert_eq!(output.stdout.lines().count(), 200);
    assert!(!output.stdout.contains("cli remote log 009"));
    assert!(output.stdout.starts_with("cli remote log 010\n"));
    assert!(output.stdout.ends_with("cli remote log 209\n"));

    let output = run_cli(
        a.path(),
        [
            "service",
            "--device",
            "b",
            "logs",
            "remote-log-demo",
            "--tail",
            "2",
        ],
    );
    assert_eq!(output.stdout, "cli remote log 208\ncli remote log 209\n");
    assert_eq!(output.stderr, "");

    let output = run_cli_result(
        a.path(),
        ["service", "logs", "remote-log-demo@b", "--tail", "2001"],
        "",
    );
    assert!(!output.status.success());
    assert_eq!(output.stdout, "");
    assert!(
        output
            .stderr
            .contains("Remote log tail must be between 1 and 2000 lines"),
        "{}",
        output.stderr
    );
}

fn init_fungi_dir(path: &std::path::Path, rpc_port: u16, swarm_port: u16) {
    run_cli(path, ["init"]);
    assert!(
        path.join("cache").join("direct_addresses.json").exists(),
        "direct address cache should persist outside devices.toml"
    );
    let mut config = FungiConfig::apply_from_dir(path).unwrap();
    config.rpc.listen_address = format!("127.0.0.1:{rpc_port}");
    config.network.listen_tcp_port = swarm_port;
    config.network.listen_udp_port = 0;
    config.network.relay_enabled = false;
    config.network.use_community_relays = false;
    config.save_to_file().unwrap();
}

fn write_existing_tcp_service_manifest(
    dir: &std::path::Path,
    name: &str,
    port: u16,
    client_kind: &str,
) -> std::path::PathBuf {
    let path = dir.join(format!("{name}.fungi.md"));
    std::fs::write(
        &path,
        format!(
            r#"---
fungi: service/v1
id: {name}
publish:
  main:
    tcp:
      host: 127.0.0.1
      port: {port}
    client:
      kind: {client_kind}
---

# {name}
"#
        ),
    )
    .unwrap();
    path
}

fn write_wasmtime_dry_run_manifest(dir: &std::path::Path) -> std::path::PathBuf {
    let component = dir.join("dry-run.wasm");
    std::fs::write(&component, b"wasm").unwrap();
    let path = dir.join("dry-run-wasi.fungi.md");
    std::fs::write(
        &path,
        format!(
            r#"---
fungi: service/v1
id: dry-run-wasi
run:
  provider: wasmtime
  source:
    file: {}
  mounts:
    - from: $fungi.root
      to: /
publish:
  main:
    tcp:
      port: 8080
    client:
      kind: raw
---

# dry-run-wasi
"#,
            component.display()
        ),
    )
    .unwrap();
    path
}

fn write_wasmtime_service_manifest(
    dir: &std::path::Path,
    component: &std::path::Path,
    name: &str,
) -> std::path::PathBuf {
    let path = dir.join(format!("{name}.fungi.md"));
    std::fs::write(
        &path,
        format!(
            r#"---
fungi: service/v1
id: {name}
run:
  provider: wasmtime
  source:
    file: {}
publish:
  main:
    tcp:
      port: 8080
---

# {name}
"#,
            component.display()
        ),
    )
    .unwrap();
    path
}

fn start_daemon(path: &std::path::Path) -> DaemonChild {
    let child = Command::new(fungi_bin())
        .arg("--fungi-dir")
        .arg(path)
        .arg("daemon")
        .arg("--exit-on-stdin-close")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    DaemonChild { child }
}

fn wait_peer_id(path: &std::path::Path) -> String {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let output = run_cli_result(path, ["info", "id"], "");
        if output.status.success() {
            return output.stdout.trim().to_string();
        }
        if Instant::now() >= deadline {
            panic!(
                "daemon did not become ready\nstdout:\n{}\nstderr:\n{}",
                output.stdout, output.stderr
            );
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn run_cli<const N: usize>(path: &std::path::Path, args: [&str; N]) -> CliOutput {
    let output = run_cli_result_with_retry(path, args, "");
    assert!(
        output.status.success(),
        "command {:?} failed\nstdout:\n{}\nstderr:\n{}",
        output.args,
        output.stdout,
        output.stderr
    );
    output
}

fn run_cli_with_input<const N: usize>(
    path: &std::path::Path,
    args: [&str; N],
    input: &str,
) -> CliOutput {
    let output = run_cli_result_with_retry(path, args, input);
    assert!(
        output.status.success(),
        "command {:?} failed\nstdout:\n{}\nstderr:\n{}",
        output.args,
        output.stdout,
        output.stderr
    );
    output
}

struct CliOutput {
    args: Vec<String>,
    status: std::process::ExitStatus,
    stdout: String,
    stderr: String,
}

fn run_cli_result_with_retry<const N: usize>(
    path: &std::path::Path,
    args: [&str; N],
    input: &str,
) -> CliOutput {
    let mut output = run_cli_result(path, args, input);
    for _ in 0..5 {
        if output.status.success() || !is_transient_grpc_transport_error(&output) {
            return output;
        }
        thread::sleep(Duration::from_millis(200));
        output = run_cli_result(path, args, input);
    }
    output
}

fn run_cli_result<const N: usize>(
    path: &std::path::Path,
    args: [&str; N],
    input: &str,
) -> CliOutput {
    let arg_list = args
        .iter()
        .map(|arg| (*arg).to_string())
        .collect::<Vec<_>>();
    let mut child = Command::new(fungi_bin())
        .arg("--fungi-dir")
        .arg(path)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    if !input.is_empty()
        && let Some(stdin) = child.stdin.as_mut()
    {
        stdin.write_all(input.as_bytes()).unwrap();
    }
    drop(child.stdin.take());

    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if child.try_wait().unwrap().is_some() {
            let output = child.wait_with_output().unwrap();
            return CliOutput {
                args: arg_list,
                status: output.status,
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            };
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let output = child.wait_with_output().unwrap();
            panic!(
                "command {:?} timed out\nstdout:\n{}\nstderr:\n{}",
                arg_list,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn is_transient_grpc_transport_error(output: &CliOutput) -> bool {
    output.stdout.trim().is_empty()
        && (output.stderr.contains("h2 protocol error")
            || output.stderr.contains("The operation was cancelled"))
}

fn connect_with_retry(addr: &str, timeout: Duration) -> TcpStream {
    let deadline = Instant::now() + timeout;
    loop {
        match TcpStream::connect(addr) {
            Ok(stream) => return stream,
            Err(error) if Instant::now() < deadline => {
                let _ = error;
                thread::sleep(Duration::from_millis(50));
            }
            Err(error) => panic!("failed to connect to {addr}: {error}"),
        }
    }
}

fn extract_local_address(output: &str) -> String {
    let mut lines = output.lines();
    while let Some(line) = lines.next() {
        if line.trim() == "local address:" {
            return lines
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| panic!("missing local address value in output:\n{output}"))
                .to_string();
        }
    }
    panic!("missing local address in output:\n{output}");
}

fn reserve_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn fungi_bin() -> &'static str {
    env!("CARGO_BIN_EXE_fungi")
}
