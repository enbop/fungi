use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use fungi_config::FungiConfig;
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

#[test]
#[cfg_attr(
    windows,
    ignore = "Windows GitHub Actions intermittently cancels short-lived local gRPC CLI connections in this two-daemon smoke; Linux/macOS cover the full remote TCP service flow"
)]
fn cli_can_create_and_access_remote_tcp_tunnel_service() {
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
        let (mut stream, _) = target.accept().unwrap();
        let mut buf = [0_u8; 4];
        stream.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"ping");
        stream.write_all(b"pong").unwrap();
    });

    run_cli_with_input(
        a.path(),
        ["service", "add", "test-tcp@b"],
        &format!("\n\n127.0.0.1:{target_port}\n\n\n\n"),
    );
    assert!(
        a.path()
            .join("cache")
            .join("remote_services")
            .join(format!("{b_peer}.json"))
            .exists(),
        "remote service cache should persist outside config.toml"
    );

    let output = run_cli(a.path(), ["test-tcp@b"]);
    let local_addr = extract_local_address(&output.stdout);
    assert!(
        a.path().join("access").join("local_access.json").exists(),
        "service access should persist outside config.toml"
    );
    let access_json =
        std::fs::read_to_string(a.path().join("access").join("local_access.json")).unwrap();
    assert!(access_json.contains("test-tcp"));
    let config_toml = std::fs::read_to_string(a.path().join("config.toml")).unwrap();
    assert!(
        !config_toml.contains("remote_service_id"),
        "service access should not be persisted in config.toml"
    );

    let mut stream = connect_with_retry(&local_addr, Duration::from_secs(5));
    stream.write_all(b"ping").unwrap();
    let mut response = [0_u8; 4];
    stream.read_exact(&mut response).unwrap();
    assert_eq!(&response, b"pong");

    server.join().unwrap();
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
    config.file_transfer.proxy_ftp.enabled = false;
    config.file_transfer.proxy_webdav.enabled = false;
    config.save_to_file().unwrap();
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
