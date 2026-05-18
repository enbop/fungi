use std::{
    fs,
    net::{TcpListener, UdpSocket},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::OnceLock,
    thread,
    time::{Duration, Instant},
};

use fungi_config::{FungiConfig, devices::DevicesConfig, trusted_devices::TrustedDevicesConfig};
use fungi_daemon::{ServicePortAllocation, ServiceSource, load_service_manifest_yaml_file};
use libp2p::PeerId;
use serde_json::json;
use tempfile::TempDir;

#[test]
#[ignore = "integration test downloads and runs the v0.6.1 release binary; run explicitly with `cargo test -p fungi --test migrate_cli -- --ignored --nocapture`"]
fn cli_migrate_upgrades_real_v061_home_with_legacy_address_book_and_service_state() {
    let home = TempDir::new().unwrap();

    run_cli(legacy_fungi_bin(), home.path(), &["init"]);

    let peer_id = PeerId::random().to_string();
    fs::write(
        home.path().join("address_book.toml"),
        format!(
            concat!(
                "[[peers]]\n",
                "peer_id = \"{peer_id}\"\n",
                "alias = \"demo-box\"\n",
                "hostname = \"demo-host\"\n",
                "private_ips = [\"192.168.0.10\"]\n",
                "os = \"MacOS\"\n",
                "version = \"0.6.1\"\n",
                "public_ip = \"203.0.113.10\"\n",
                "created_at = {{ secs_since_epoch = 1704164645, nanos_since_epoch = 0 }}\n",
                "last_connected = {{ secs_since_epoch = 1704254706, nanos_since_epoch = 0 }}\n"
            ),
            peer_id = peer_id,
        ),
    )
    .unwrap();

    let old_service_dir = home.path().join("services").join("demo");
    fs::create_dir_all(old_service_dir.join("cache")).unwrap();
    fs::write(old_service_dir.join("component.wasm"), b"wasm").unwrap();
    fs::write(old_service_dir.join("cache").join("state.txt"), b"persist").unwrap();
    fs::write(
        home.path().join("services-state.json"),
        serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "updated_at": "2026-05-01T00:00:00Z",
            "services": {
                "demo": {
                    "manifest": {
                        "name": "demo",
                        "runtime": "wasmtime",
                        "source": {
                            "WasmtimeFile": {
                                "component": old_service_dir.join("component.wasm").display().to_string()
                            }
                        },
                        "expose": {
                            "service_id": "demo-service",
                            "display_name": "Demo Service",
                            "transport": {
                                "kind": "tcp"
                            },
                            "usage": {
                                "kind": "web",
                                "path": "/"
                            },
                            "icon_url": "https://example.com/icon.png",
                            "catalog_id": "demo/catalog"
                        },
                        "env": {},
                        "mounts": [
                            {
                                "host_path": old_service_dir.join("cache").display().to_string(),
                                "runtime_path": "/cache"
                            }
                        ],
                        "ports": [
                            {
                                "name": "http",
                                "host_port": 18080,
                                "service_port": 80,
                                "protocol": "tcp"
                            }
                        ],
                        "command": [],
                        "entrypoint": [],
                        "working_dir": old_service_dir.display().to_string(),
                        "labels": {
                            "demo": "1"
                        }
                    },
                    "desired_state": "stopped"
                }
            }
        }))
        .unwrap(),
    )
    .unwrap();

    let migrate_output = run_cli(current_fungi_bin(), home.path(), &["migrate"]);
    assert!(migrate_output.stdout.contains("Migrated Fungi"));

    let config = FungiConfig::apply_from_dir(home.path()).unwrap();
    assert_eq!(config.version, 2);
    assert!(config.runtime.allowed_host_paths.is_empty());
    let raw_config = fs::read_to_string(home.path().join("config.toml")).unwrap();
    assert!(raw_config.contains("version = 2"));
    assert!(!raw_config.contains("allowed_port_ranges"));
    assert!(!raw_config.contains("allowed_ports"));
    assert!(!raw_config.contains(&home.path().join("services").display().to_string()));

    let devices = DevicesConfig::apply_from_dir(home.path()).unwrap();
    assert_eq!(devices.devices.len(), 1);
    assert_eq!(devices.devices[0].peer_id.to_string(), peer_id);
    assert_eq!(devices.devices[0].name.as_deref(), Some("demo-box"));
    assert_eq!(devices.devices[0].hostname.as_deref(), Some("demo-host"));
    assert_eq!(devices.devices[0].multiaddrs.len(), 0);
    assert_eq!(devices.devices[0].private_ips, vec!["192.168.0.10"]);
    assert_eq!(
        devices.devices[0].public_ip.as_deref(),
        Some("203.0.113.10")
    );
    assert!(!home.path().join("address_book.toml").exists());

    let backup_entries = fs::read_dir(home.path().join("bk"))
        .unwrap()
        .filter_map(|entry| entry.ok())
        .collect::<Vec<_>>();
    assert_eq!(backup_entries.len(), 1);
    let backup_dir = backup_entries[0].path();
    assert!(backup_dir.join("config.toml").is_file());
    assert!(backup_dir.join("address_book.toml").is_file());
    assert!(backup_dir.join("services-state.json").is_file());
    assert!(backup_dir.join("services").join("demo").is_dir());
    assert!(!backup_dir.join(".keys").exists());

    let staging_count = fs::read_dir(home.path())
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(".fungi-migrate-staging-")
        })
        .count();
    assert_eq!(staging_count, 0);

    assert!(!home.path().join("services-state.json").exists());
    let service_entries = fs::read_dir(home.path().join("services"))
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().unwrap().is_dir())
        .collect::<Vec<_>>();
    assert_eq!(service_entries.len(), 1);
    let local_service_id = service_entries[0].file_name().to_string_lossy().to_string();
    assert!(local_service_id.starts_with("svc_"));
    assert!(!home.path().join("services").join("demo").exists());

    let data_dir = home.path().join("data").join(&local_service_id);
    assert!(data_dir.is_dir());
    assert!(data_dir.join("component.wasm").is_file());
    assert_eq!(
        fs::read_to_string(data_dir.join("cache").join("state.txt")).unwrap(),
        "persist"
    );

    let manifest_path = home
        .path()
        .join("services")
        .join(&local_service_id)
        .join("service.yaml");
    let manifest_yaml = fs::read_to_string(&manifest_path).unwrap();
    assert!(!manifest_yaml.contains("serviceId"));
    assert!(!manifest_yaml.contains("displayName"));
    assert!(!manifest_yaml.contains(&old_service_dir.display().to_string()));
    assert!(manifest_yaml.contains(&data_dir.display().to_string()));

    let manifest = load_service_manifest_yaml_file(&manifest_path, home.path()).unwrap();
    assert_eq!(manifest.name, "demo");
    assert_eq!(
        manifest.working_dir.as_deref(),
        Some(data_dir.to_string_lossy().as_ref())
    );
    assert_eq!(manifest.mounts.len(), 1);
    assert_eq!(manifest.mounts[0].host_path, data_dir.join("cache"));
    match &manifest.source {
        ServiceSource::WasmtimeFile { component } => {
            assert_eq!(component, &data_dir.join("component.wasm"));
        }
        other => panic!("unexpected migrated manifest source: {other:?}"),
    }

    let state_value: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(
            home.path()
                .join("services")
                .join(&local_service_id)
                .join("state.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(state_value["schema_version"], 2);
    assert_eq!(state_value["local_service_id"], local_service_id);
    assert_eq!(state_value["desired_state"], "stopped");

    let second_migrate = run_cli(current_fungi_bin(), home.path(), &["migrate"]);
    assert!(second_migrate.stdout.contains("already at version 2"));
}

#[test]
#[ignore = "integration test runs the v0.6.1 release daemon and current daemon; run explicitly with `cargo test -p fungi --test migrate_cli -- --ignored --nocapture`"]
fn cli_migrate_preserves_v061_daemon_cli_written_config_and_service_state() {
    let home = TempDir::new().unwrap();
    let payload = TempDir::new().unwrap();
    let legacy = legacy_fungi_bin();
    let current = current_fungi_bin();

    run_cli(legacy, home.path(), &["init"]);
    configure_legacy_daemon_ports(home.path());

    let peer_id = PeerId::random().to_string();
    let listen_port = reserve_tcp_port();
    let forward_port = reserve_tcp_port();
    let allowed_port = reserve_tcp_port();
    let service_host_port = reserve_tcp_port();
    let legacy_service_dir = home.path().join("services").join("cli-demo");
    fs::create_dir_all(legacy_service_dir.join("mount")).unwrap();
    fs::write(legacy_service_dir.join("component.wasm"), b"wasm").unwrap();
    fs::write(
        legacy_service_dir.join("mount").join("state.txt"),
        b"persist",
    )
    .unwrap();
    let manifest_path = payload.path().join("cli-demo-service.yaml");
    fs::write(
        &manifest_path,
        format!(
            r#"apiVersion: fungi.rs/v1alpha1
kind: ServiceManifest
metadata:
  name: cli-demo
  labels:
    release-test: "1"
spec:
  runtime: wasmtime
  source:
    file: $APP_HOME/component.wasm
  expose:
    enabled: true
    serviceId: cli-demo-service
    displayName: CLI Demo
    transport:
      kind: tcp
    usage:
      kind: web
      path: /ui
    iconUrl: https://example.com/icon.png
    catalogId: example/cli-demo
  ports:
    - name: http
      hostPort: {service_host_port}
      servicePort: 8080
      protocol: tcp
  mounts:
    - hostPath: $APP_HOME/mount
      runtimePath: /data
  command:
    - --demo
  entrypoint: []
  workingDir: $APP_HOME/mount
"#
        ),
    )
    .unwrap();

    {
        let _daemon = start_daemon(legacy, home.path());
        run_cli(legacy, home.path(), &["security", "allow-path", "/tmp"]);
        run_cli(
            legacy,
            home.path(),
            &["security", "allow-port", &allowed_port.to_string()],
        );
        run_cli(
            legacy,
            home.path(),
            &["security", "allow-range", "19100", "19110"],
        );
        run_cli(
            legacy,
            home.path(),
            &[
                "security",
                "allowed-peers",
                "add",
                "--alias",
                "demo-peer",
                &peer_id,
            ],
        );
        run_cli(
            legacy,
            home.path(),
            &["tunnel", "add-listen", &format!("127.0.0.1:{listen_port}")],
        );
        run_cli(
            legacy,
            home.path(),
            &[
                "tunnel",
                "add-forward",
                &format!("127.0.0.1:{forward_port}"),
                &peer_id,
                &listen_port.to_string(),
            ],
        );
        run_cli(
            legacy,
            home.path(),
            &["ft-client", "add", "--name", "files", "--enabled", &peer_id],
        );
        run_cli(
            legacy,
            home.path(),
            &[
                "ft-client",
                "ftp-update",
                "--enabled",
                "--host",
                "127.0.0.1",
                "--port",
                "2122",
            ],
        );
        run_cli(
            legacy,
            home.path(),
            &[
                "ft-client",
                "webdav-update",
                "--enabled",
                "--host",
                "127.0.0.1",
                "--port",
                "8182",
            ],
        );
        run_cli(
            legacy,
            home.path(),
            &["service", "pull", manifest_path.to_str().unwrap()],
        );
    }

    let legacy_config = fs::read_to_string(home.path().join("config.toml")).unwrap();
    assert!(legacy_config.contains("incoming_allowed_peers"));
    assert!(legacy_config.contains("allowed_ports"));
    assert!(home.path().join("address_book.toml").exists());
    assert!(home.path().join("services-state.json").exists());

    let migrate_output = run_cli(current, home.path(), &["migrate"]);
    assert!(migrate_output.stdout.contains("Migrated Fungi"));

    let migrated_config_raw = fs::read_to_string(home.path().join("config.toml")).unwrap();
    assert!(migrated_config_raw.contains("version = 2"));
    assert!(!migrated_config_raw.contains("incoming_allowed_peers"));
    assert!(!migrated_config_raw.contains("allowed_ports"));
    assert!(!migrated_config_raw.contains("allowed_port_ranges"));
    assert!(!migrated_config_raw.contains(&home.path().join("services").display().to_string()));
    assert!(migrated_config_raw.contains("\"/tmp\""));

    let config = FungiConfig::apply_from_dir(home.path()).unwrap();
    assert_eq!(config.version, 2);
    assert_eq!(
        config.runtime.allowed_host_paths,
        vec![PathBuf::from("/tmp")]
    );
    assert_eq!(config.tcp_tunneling.listening.rules.len(), 1);
    assert_eq!(config.tcp_tunneling.listening.rules[0].port, listen_port);
    assert_eq!(config.tcp_tunneling.forwarding.rules.len(), 1);
    assert_eq!(
        config.tcp_tunneling.forwarding.rules[0].local_port,
        forward_port
    );
    assert_eq!(config.file_transfer.client.len(), 1);
    assert_eq!(
        config.file_transfer.client[0].name.as_deref(),
        Some("files")
    );
    assert!(config.file_transfer.client[0].enabled);
    assert_eq!(config.file_transfer.client[0].peer_id.to_string(), peer_id);
    assert!(config.file_transfer.proxy_ftp.enabled);
    assert_eq!(config.file_transfer.proxy_ftp.port, 2122);
    assert!(config.file_transfer.proxy_webdav.enabled);
    assert_eq!(config.file_transfer.proxy_webdav.port, 8182);

    let trusted = TrustedDevicesConfig::apply_from_dir(home.path()).unwrap();
    assert_eq!(trusted.trusted_devices.len(), 1);
    assert_eq!(trusted.trusted_devices[0].to_string(), peer_id);

    let devices = DevicesConfig::apply_from_dir(home.path()).unwrap();
    assert_eq!(devices.devices.len(), 1);
    assert_eq!(devices.devices[0].peer_id.to_string(), peer_id);
    assert_eq!(devices.devices[0].name.as_deref(), Some("demo-peer"));
    assert!(!home.path().join("address_book.toml").exists());

    assert!(!home.path().join("services-state.json").exists());
    let service_entries = fs::read_dir(home.path().join("services"))
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().unwrap().is_dir())
        .collect::<Vec<_>>();
    assert_eq!(service_entries.len(), 1);
    let local_service_id = service_entries[0].file_name().to_string_lossy().to_string();
    assert!(local_service_id.starts_with("svc_"));
    assert!(!home.path().join("services").join("cli-demo").exists());

    let data_dir = home.path().join("data").join(&local_service_id);
    assert!(data_dir.join("component.wasm").is_file());
    assert_eq!(
        fs::read_to_string(data_dir.join("mount").join("state.txt")).unwrap(),
        "persist"
    );

    let migrated_manifest_path = home
        .path()
        .join("services")
        .join(&local_service_id)
        .join("service.yaml");
    let migrated_manifest_yaml = fs::read_to_string(&migrated_manifest_path).unwrap();
    assert!(migrated_manifest_yaml.contains(&format!("hostPort: {service_host_port}")));
    assert!(!migrated_manifest_yaml.contains(&legacy_service_dir.display().to_string()));

    let manifest = load_service_manifest_yaml_file(&migrated_manifest_path, home.path()).unwrap();
    assert_eq!(manifest.name, "cli-demo");
    assert_eq!(manifest.ports[0].host_port, service_host_port);
    assert_eq!(
        manifest.ports[0].host_port_allocation,
        ServicePortAllocation::Fixed
    );
    assert_eq!(manifest.mounts[0].host_path, data_dir.join("mount"));
    match &manifest.source {
        ServiceSource::WasmtimeFile { component } => {
            assert_eq!(component, &data_dir.join("component.wasm"));
        }
        other => panic!("unexpected migrated manifest source: {other:?}"),
    }

    {
        let _daemon = start_daemon(current, home.path());
        let inspect = run_cli(current, home.path(), &["service", "inspect", "cli-demo"]);
        assert!(inspect.stdout.contains("\"name\": \"cli-demo\""));
        assert!(inspect.stdout.contains("\"running\": false"));
    }
}

fn current_fungi_bin() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_fungi"))
}

fn legacy_fungi_bin() -> &'static Path {
    static LEGACY_BIN: OnceLock<PathBuf> = OnceLock::new();
    LEGACY_BIN.get_or_init(|| {
        let asset_name = legacy_asset_name();
        let cache_root = std::env::temp_dir()
            .join("fungi-cli-release-cache")
            .join("v0.6.1")
            .join(asset_name.trim_end_matches(".tar.gz"));
        let binary_name = if cfg!(target_os = "windows") {
            "fungi.exe"
        } else {
            "fungi"
        };
        let binary_path = cache_root.join(binary_name);
        if binary_path.exists() {
            return binary_path;
        }

        fs::create_dir_all(&cache_root).unwrap();
        let archive_path = cache_root.join(asset_name);
        let url = format!("https://github.com/enbop/fungi/releases/download/v0.6.1/{asset_name}");

        run_process(
            Command::new("curl")
                .arg("-L")
                .arg("-f")
                .arg("--retry")
                .arg("3")
                .arg("-o")
                .arg(&archive_path)
                .arg(url),
            "download v0.6.1 fungi release asset",
        );
        run_process(
            Command::new("tar")
                .current_dir(&cache_root)
                .arg("-xzf")
                .arg(&archive_path),
            "extract v0.6.1 fungi release asset",
        );
        if !cfg!(target_os = "windows") {
            run_process(
                Command::new("chmod").arg("+x").arg(&binary_path),
                "mark extracted fungi binary executable",
            );
        }
        assert!(
            binary_path.exists(),
            "legacy fungi binary was not extracted"
        );
        binary_path
    })
}

fn legacy_asset_name() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "fungi-macos-aarch64.tar.gz",
        ("macos", "x86_64") => "fungi-macos-x86_64.tar.gz",
        ("linux", "aarch64") => "fungi-linux-aarch64.tar.gz",
        ("linux", "x86_64") => "fungi-linux-x86_64.tar.gz",
        _ => panic!(
            "unsupported platform for legacy fungi CLI migration test: {} {}",
            std::env::consts::OS,
            std::env::consts::ARCH
        ),
    }
}

fn run_process(command: &mut Command, description: &str) {
    let output = command.output().unwrap_or_else(|error| {
        panic!("failed to {description}: {error}");
    });
    assert!(
        output.status.success(),
        "failed to {description}\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

struct CliOutput {
    stdout: String,
}

struct RunningDaemon {
    child: Child,
    _stdin: ChildStdin,
}

impl Drop for RunningDaemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn configure_legacy_daemon_ports(fungi_dir: &Path) {
    let config_path = fungi_dir.join("config.toml");
    let rpc_port = reserve_tcp_port();
    let tcp_port = reserve_tcp_port();
    let udp_port = reserve_udp_port();
    let content = fs::read_to_string(&config_path).unwrap();
    let content = content
        .replace(
            "listen_address = \"127.0.0.1:5405\"",
            &format!("listen_address = \"127.0.0.1:{rpc_port}\""),
        )
        .replace(
            "listen_tcp_port = 0",
            &format!("listen_tcp_port = {tcp_port}"),
        )
        .replace(
            "listen_udp_port = 0",
            &format!("listen_udp_port = {udp_port}"),
        );
    fs::write(config_path, content).unwrap();
}

fn reserve_tcp_port() -> u16 {
    TcpListener::bind(("127.0.0.1", 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn reserve_udp_port() -> u16 {
    UdpSocket::bind(("127.0.0.1", 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn start_daemon(binary: &Path, fungi_dir: &Path) -> RunningDaemon {
    let mut child = Command::new(binary)
        .arg("--fungi-dir")
        .arg(fungi_dir)
        .arg("daemon")
        .arg("--exit-on-stdin-close")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let stdin = child.stdin.take().unwrap();
    let daemon = RunningDaemon {
        child,
        _stdin: stdin,
    };

    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if try_run_cli(binary, fungi_dir, &["info", "version"]).is_some() {
            return daemon;
        }
        if Instant::now() >= deadline {
            panic!("daemon did not become ready");
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn run_cli(binary: &Path, fungi_dir: &Path, args: &[&str]) -> CliOutput {
    let mut child = Command::new(binary)
        .arg("--fungi-dir")
        .arg(fungi_dir)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if child.try_wait().unwrap().is_some() {
            let output = child.wait_with_output().unwrap();
            let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
            assert!(
                output.status.success(),
                "command failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
            );
            return CliOutput { stdout };
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let output = child.wait_with_output().unwrap();
            panic!(
                "command timed out\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn try_run_cli(binary: &Path, fungi_dir: &Path, args: &[&str]) -> Option<CliOutput> {
    let output = Command::new(binary)
        .arg("--fungi-dir")
        .arg(fungi_dir)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(CliOutput {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
    })
}
