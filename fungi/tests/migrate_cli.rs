use std::{
    fs,
    net::{TcpListener, UdpSocket},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::OnceLock,
    thread,
    time::{Duration, Instant},
};

use fungi_config::{FungiConfig, devices::DevicesConfig};
use fungi_daemon::load_service_manifest_yaml_file;
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
    assert_eq!(config.version, 3);
    assert!(config.runtime.allowed_host_paths.is_empty());
    let raw_config = fs::read_to_string(home.path().join("config.toml")).unwrap();
    assert!(raw_config.contains("version = 3"));
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
    assert!(backup_dir.join(".keys").join("keypair").is_file());

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

    let appdata_dir = home
        .path()
        .join("appdata")
        .join("services")
        .join(&local_service_id);
    let artifacts_dir = home
        .path()
        .join("artifacts")
        .join("services")
        .join(&local_service_id);
    assert!(appdata_dir.is_dir());
    assert!(artifacts_dir.join("component.wasm").is_file());
    assert_eq!(
        fs::read_to_string(appdata_dir.join("cache").join("state.txt")).unwrap(),
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
    assert!(manifest_yaml.contains("fungi: service/v1"));
    assert!(manifest_yaml.contains("$fungi.service.data/cache"));
    assert!(manifest_yaml.contains("$fungi.service.artifacts/component.wasm"));

    assert!(load_service_manifest_yaml_file(&manifest_path, home.path()).is_err());
    let manifest: serde_yaml::Value = serde_yaml::from_str(&manifest_yaml).unwrap();
    assert_eq!(manifest["instance"], "demo");
    assert!(manifest["id"].is_null());
    assert_eq!(manifest["publish"]["http"]["tcp"]["port"], 18080);

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
    assert!(
        state_value["configuration_error"]
            .as_str()
            .unwrap()
            .contains("run-compatible")
    );

    let second_migrate = run_cli(current_fungi_bin(), home.path(), &["migrate"]);
    assert!(second_migrate.stdout.contains("already at version 3"));
}

#[test]
#[ignore = "integration test runs the v0.6.1 release daemon and current daemon; run explicitly with `cargo test -p fungi --test migrate_cli -- --ignored --nocapture`"]
fn cli_migrate_preserves_v061_daemon_written_state_in_current_layout() {
    let home = TempDir::new().unwrap();
    let payload = TempDir::new().unwrap();
    let legacy = legacy_fungi_bin();
    let current = current_fungi_bin();

    run_cli(legacy, home.path(), &["init"]);
    configure_legacy_daemon_ports(home.path());

    let peer_id = PeerId::random().to_string();
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
            &["service", "pull", manifest_path.to_str().unwrap()],
        );
    }

    let legacy_config = fs::read_to_string(home.path().join("config.toml")).unwrap();
    assert!(legacy_config.contains("incoming_allowed_peers"));
    assert!(legacy_config.contains("tcp_tunneling"));
    assert!(legacy_config.contains("file_transfer"));
    assert!(home.path().join("address_book.toml").exists());
    assert!(home.path().join("services-state.json").exists());

    let migrate_output = run_cli(current, home.path(), &["migrate"]);
    assert!(migrate_output.stdout.contains("Migrated Fungi"));

    let migrated_config_raw = fs::read_to_string(home.path().join("config.toml")).unwrap();
    assert!(migrated_config_raw.contains("version = 3"));
    assert!(!migrated_config_raw.contains("incoming_allowed_peers"));
    assert!(!migrated_config_raw.contains("tcp_tunneling"));
    assert!(!migrated_config_raw.contains("file_transfer"));
    assert!(!migrated_config_raw.contains("allowed_ports"));
    assert!(!migrated_config_raw.contains("allowed_port_ranges"));
    assert!(migrated_config_raw.contains("\"/tmp\""));

    let config = FungiConfig::apply_from_dir(home.path()).unwrap();
    assert_eq!(config.version, 3);
    assert_eq!(
        config.runtime.allowed_host_paths,
        vec![PathBuf::from("/tmp")]
    );

    let trusted =
        fungi_config::trusted_devices::TrustedDevicesConfig::apply_from_dir(home.path()).unwrap();
    assert_eq!(trusted.trusted_devices.len(), 1);
    assert_eq!(trusted.trusted_devices[0].to_string(), peer_id);

    let devices = DevicesConfig::apply_from_dir(home.path()).unwrap();
    assert_eq!(devices.devices.len(), 1);
    assert_eq!(devices.devices[0].peer_id.to_string(), peer_id);
    assert_eq!(devices.devices[0].name.as_deref(), Some("demo-peer"));

    let service_entries = fs::read_dir(home.path().join("services"))
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().unwrap().is_dir())
        .collect::<Vec<_>>();
    assert_eq!(service_entries.len(), 1);
    let local_service_id = service_entries[0].file_name().to_string_lossy().to_string();
    let appdata_dir = home
        .path()
        .join("appdata")
        .join("services")
        .join(&local_service_id);
    let artifacts_dir = home
        .path()
        .join("artifacts")
        .join("services")
        .join(&local_service_id);
    assert_eq!(
        fs::read_to_string(appdata_dir.join("mount").join("state.txt")).unwrap(),
        "persist"
    );
    assert!(artifacts_dir.join("component.wasm").is_file());
    assert!(!appdata_dir.join("component.wasm").exists());

    let migrated_manifest_path = home
        .path()
        .join("services")
        .join(&local_service_id)
        .join("service.yaml");
    let migrated_manifest_yaml = fs::read_to_string(&migrated_manifest_path).unwrap();
    assert!(migrated_manifest_yaml.contains("fungi: service/v1"));
    assert!(migrated_manifest_yaml.contains(&format!("port: {service_host_port}")));
    assert!(migrated_manifest_yaml.contains("$fungi.service.data/mount"));
    assert!(migrated_manifest_yaml.contains("$fungi.service.artifacts/component.wasm"));

    let backup_entries = fs::read_dir(home.path().join("bk"))
        .unwrap()
        .filter_map(|entry| entry.ok())
        .collect::<Vec<_>>();
    assert_eq!(backup_entries.len(), 1);
    assert!(
        backup_entries[0]
            .path()
            .join(".keys")
            .join("keypair")
            .is_file()
    );

    {
        let _daemon = start_daemon(current, home.path());
        let inspect = run_cli(current, home.path(), &["service", "inspect", "cli-demo"]);
        assert!(inspect.stdout.contains("\"name\": \"cli-demo\""));
        assert!(inspect.stdout.contains("\"phase\": \"unknown\""));
        assert!(inspect.stdout.contains("configuration error"));
        assert!(inspect.stdout.contains("run-compatible"));
    }

    let second_migrate = run_cli(current, home.path(), &["migrate"]);
    assert!(second_migrate.stdout.contains("already at version 3"));
}

fn current_fungi_bin() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_fungi"))
}

#[test]
fn cli_apply_recovers_after_persistence_failure_without_daemon_restart() {
    let home = TempDir::new().unwrap();
    let binary = current_fungi_bin();
    run_cli(binary, home.path(), &["init"]);
    let component = home.path().join("component.wasm");
    fs::write(&component, b"component used only for apply").unwrap();
    let manifest = home.path().join("retryable.yaml");
    fs::write(&manifest, format!(
        "fungi: service/v1\nid: retryable\nrun:\n  provider: wasmtime\n  source:\n    file: {}\npublish:\n  main:\n    tcp:\n      port: {}\n",
        component.display(), reserve_tcp_port()
    )).unwrap();
    let _daemon = start_daemon(binary, home.path());
    let services = home.path().join("services");
    fs::remove_dir(&services).unwrap();
    fs::write(&services, b"blocked").unwrap();
    let apply_args = [
        "service",
        "apply",
        "retryable",
        manifest.to_str().unwrap(),
        "--yes",
    ];
    assert!(try_run_cli(binary, home.path(), &apply_args).is_none());
    assert!(try_run_cli(binary, home.path(), &["service", "inspect", "retryable"]).is_none());
    fs::remove_file(&services).unwrap();
    fs::create_dir(&services).unwrap();
    run_cli(binary, home.path(), &apply_args);
    let inspect = run_cli(binary, home.path(), &["service", "inspect", "retryable"]);
    assert!(
        inspect.stdout.contains("\"phase\": \"stopped\""),
        "{}",
        inspect.stdout
    );
    run_cli(binary, home.path(), &["service", "remove", "retryable"]);
    assert_eq!(fs::read_dir(&services).unwrap().count(), 0);
}

#[test]
fn cli_keeps_legacy_http_services_manageable_after_upgrade() {
    for legacy_layout in [false, true] {
        let home = TempDir::new().unwrap();
        let binary = current_fungi_bin();
        run_cli(binary, home.path(), &["init"]);
        let port = reserve_tcp_port();
        let upgraded_component = home.path().join("upgraded.wasm");
        fs::write(&upgraded_component, b"replacement component").unwrap();
        let manifest = format!(
            "fungi: service/v1\nid: official-recipe\ninstance: demo\nrun:\n  provider: wasmtime\n  source:\n    file: {}\npublish:\n  http:\n    tcp:\n      port: {port}\n",
            upgraded_component.display()
        );
        if legacy_layout {
            let old_dir = home.path().join("services/demo");
            fs::create_dir_all(&old_dir).unwrap();
            fs::write(old_dir.join("keep.txt"), "user data").unwrap();
            fs::write(home.path().join("services-state.json"), serde_json::to_vec(&json!({
                "schema_version": 1, "services": {"demo": {
                    "manifest": {"name": "demo", "runtime": "wasmtime",
                        "source": {"WasmtimeFile": {"component": upgraded_component}},
                        "ports": [{"host_port": port, "service_port": port, "protocol": "tcp"}]},
                    "desired_state": "running"
                }}
            })).unwrap()).unwrap();
        } else {
            let saved = home.path().join("services/svc_old");
            fs::create_dir_all(&saved).unwrap();
            fs::write(
                saved.join("service.yaml"),
                manifest.replace("  provider: wasmtime", "  provider: wasmtime\n  mode: http"),
            )
            .unwrap();
            fs::write(
                saved.join("state.json"),
                r#"{"schema_version":2,"local_service_id":"svc_old","desired_state":"running"}"#,
            )
            .unwrap();
            let data = home.path().join("appdata/services/svc_old");
            fs::create_dir_all(&data).unwrap();
            fs::write(data.join("keep.txt"), "user data").unwrap();
        }
        let upgraded_manifest = home.path().join("upgraded.service.yaml");
        fs::write(&upgraded_manifest, &manifest).unwrap();
        run_cli(binary, home.path(), &["migrate"]);
        let daemon = start_daemon(binary, home.path());
        let inspect = run_cli(binary, home.path(), &["service", "inspect", "demo"]);
        assert!(
            inspect.stdout.contains("configuration error"),
            "{}",
            inspect.stdout
        );
        assert!(inspect.stdout.contains("run-compatible"));
        assert!(try_run_cli(binary, home.path(), &["service", "start", "demo"]).is_none());
        let entries = fs::read_dir(home.path().join("services"))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(entries.len(), 1);
        let id = entries[0].file_name();
        let data = home
            .path()
            .join("appdata/services")
            .join(&id)
            .join("keep.txt");
        run_cli(
            binary,
            home.path(),
            &[
                "service",
                "apply",
                "demo",
                upgraded_manifest.to_str().unwrap(),
                "--yes",
            ],
        );
        assert_eq!(fs::read_to_string(&data).unwrap(), "user data");
        assert!(
            !fs::read_to_string(entries[0].path().join("service.yaml"))
                .unwrap()
                .contains("mode:")
        );
        assert!(
            !fs::read_to_string(entries[0].path().join("state.json"))
                .unwrap()
                .contains("configuration_error")
        );
        assert_eq!(
            fs::read_dir(home.path().join("services")).unwrap().count(),
            1
        );
        drop(daemon);
        let _restarted = start_daemon(binary, home.path());
        let inspect = run_cli(binary, home.path(), &["service", "inspect", "demo"]);
        assert!(
            inspect.stdout.contains("\"phase\": \"stopped\""),
            "{}",
            inspect.stdout
        );
    }
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
    let stderr_path = fungi_dir.join("test-daemon.stderr");
    let mut child = Command::new(binary)
        .arg("--fungi-dir")
        .arg(fungi_dir)
        .arg("daemon")
        .arg("--exit-on-stdin-close")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(fs::File::create(&stderr_path).unwrap())
        .spawn()
        .unwrap();
    let stdin = child.stdin.take().unwrap();
    let mut daemon = RunningDaemon {
        child,
        _stdin: stdin,
    };

    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(status) = daemon.child.try_wait().unwrap() {
            panic!(
                "daemon exited with {status}: {}",
                fs::read_to_string(&stderr_path).unwrap()
            );
        }
        if try_run_cli(binary, fungi_dir, &["info", "version"]).is_some() {
            return daemon;
        }
        if Instant::now() >= deadline {
            panic!(
                "daemon did not become ready: {}",
                fs::read_to_string(&stderr_path).unwrap()
            );
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
