use super::*;
use crate::service_state::DesiredServiceState;
use anyhow::Result;
use fungi_config::paths::FungiPaths;
use fungi_docker_agent::DockerAgentError;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    net::{SocketAddr, TcpListener as StdTcpListener},
    path::{Path, PathBuf},
};
use tempfile::TempDir;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    time::{Duration, sleep},
};

use super::helpers::{
    build_wasmtime_command, docker_spec_from_manifest_with_name, ensure_manifest_mount_dirs,
    is_missing_docker_container_error,
};
use super::providers::WasmtimeServiceState;

#[test]
fn docker_manifest_maps_to_container_spec() {
    let manifest = ServiceManifest {
        name: "filebrowser".into(),
        definition_id: None,
        runtime: RuntimeKind::Docker,
        source: ServiceSource::Docker {
            image: "filebrowser/filebrowser:latest".into(),
        },
        expose: None,
        env: BTreeMap::from([(String::from("FB_NOAUTH"), String::from("true"))]),
        mounts: vec![ServiceMount {
            host_path: PathBuf::from("/tmp/fungi/data"),
            runtime_path: "/srv".into(),
        }],
        ports: vec![ServicePort {
            name: None,
            host_port: 8080,
            host_port_allocation: ServicePortAllocation::Fixed,
            service_port: 80,
            protocol: ServicePortProtocol::Tcp,
        }],
        command: vec!["serve".into()],
        entrypoint: Vec::new(),
        working_dir: None,
        labels: BTreeMap::new(),
    };

    let spec = docker_spec_from_manifest_with_name(&manifest, &manifest.name).unwrap();
    assert_eq!(spec.name.as_deref(), Some("filebrowser"));
    assert_eq!(spec.image, "filebrowser/filebrowser:latest");
    assert_eq!(spec.ports[0].host_port, 8080);
}

#[test]
fn docker_manifest_can_use_internal_container_name() {
    let manifest = ServiceManifest {
        name: "c".into(),
        definition_id: Some("code-server".into()),
        runtime: RuntimeKind::Docker,
        source: ServiceSource::Docker {
            image: "ghcr.io/coder/code-server:latest".into(),
        },
        expose: None,
        env: BTreeMap::new(),
        mounts: Vec::new(),
        ports: Vec::new(),
        command: Vec::new(),
        entrypoint: Vec::new(),
        working_dir: None,
        labels: BTreeMap::new(),
    };

    let spec =
        docker_spec_from_manifest_with_name(&manifest, "svc_01hz7j7n3evh1q4j1a8g9c2d3e").unwrap();

    assert_eq!(spec.name.as_deref(), Some("svc_01hz7j7n3evh1q4j1a8g9c2d3e"));
}

#[test]
fn ensure_manifest_mount_dirs_creates_missing_host_paths() {
    let temp_dir = TempDir::new().unwrap();
    let mount_path = temp_dir.path().join("nested/data");
    let manifest = ServiceManifest {
        name: "mount-test".into(),
        definition_id: None,
        runtime: RuntimeKind::Wasmtime,
        source: ServiceSource::WasmtimeFile {
            component: temp_dir.path().join("demo.wasm"),
        },
        expose: None,
        env: BTreeMap::new(),
        mounts: vec![ServiceMount {
            host_path: mount_path.clone(),
            runtime_path: "data".into(),
        }],
        ports: Vec::new(),
        command: Vec::new(),
        entrypoint: Vec::new(),
        working_dir: None,
        labels: BTreeMap::new(),
    };

    ensure_manifest_mount_dirs(&manifest).unwrap();
    assert!(mount_path.is_dir());
}

#[test]
fn runtime_control_new_creates_services_root() {
    let temp_dir = TempDir::new().unwrap();
    let fungi_home = temp_dir.path().join("fungi-home");
    let paths = FungiPaths::from_fungi_home(&fungi_home);
    let runtime_root = fungi_home.join("runtime");
    let services_root = fungi_home.join("services");

    RuntimeControl::new(
        runtime_root,
        PathBuf::from("/bin/echo"),
        fungi_home.clone(),
        None,
        services_root,
        Vec::new(),
        false,
    )
    .unwrap();

    assert!(fungi_home.join("services").is_dir());
    assert!(fungi_home.join("appdata/services").is_dir());
    assert!(fungi_home.join("artifacts/services").is_dir());
    assert!(paths.user_home().is_dir());
}

#[test]
fn docker_manifest_rejects_wrong_source_type() {
    let manifest = ServiceManifest {
        name: "bad".into(),
        definition_id: None,
        runtime: RuntimeKind::Docker,
        source: ServiceSource::WasmtimeFile {
            component: PathBuf::from("/tmp/app.wasm"),
        },
        expose: None,
        env: BTreeMap::new(),
        mounts: Vec::new(),
        ports: Vec::new(),
        command: Vec::new(),
        entrypoint: Vec::new(),
        working_dir: None,
        labels: BTreeMap::new(),
    };

    assert!(docker_spec_from_manifest_with_name(&manifest, &manifest.name).is_err());
}

#[tokio::test]
async fn wasmtime_provider_runs_fake_launcher_and_collects_logs() {
    let temp_dir = TempDir::new().unwrap();
    let launcher = create_fake_launcher(temp_dir.path()).unwrap();
    let component = temp_dir.path().join("demo.wasm");
    fs::write(&component, b"wasm-bytes").unwrap();

    let provider = WasmtimeRuntimeProvider::new(
        temp_dir.path().join("runtime"),
        launcher,
        temp_dir.path().to_path_buf(),
        vec![temp_dir.path().to_path_buf()],
    );
    let manifest = ServiceManifest {
        name: "demo-service".into(),
        definition_id: None,
        runtime: RuntimeKind::Wasmtime,
        source: ServiceSource::WasmtimeFile {
            component: component.clone(),
        },
        expose: Some(ServiceExpose {
            transport: ServiceExposeTransport {
                kind: ServiceExposeTransportKind::Tcp,
            },
            usage: Some(ServiceExposeUsage {
                kind: ServiceExposeUsageKind::Web,
                path: Some("/".into()),
            }),
            icon_url: None,
        }),
        env: BTreeMap::new(),
        mounts: vec![ServiceMount {
            host_path: temp_dir.path().join("data"),
            runtime_path: "data".into(),
        }],
        ports: vec![ServicePort {
            name: None,
            host_port: 18081,
            host_port_allocation: ServicePortAllocation::Fixed,
            service_port: 8081,
            protocol: ServicePortProtocol::Tcp,
        }],
        command: Vec::new(),
        entrypoint: Vec::new(),
        working_dir: None,
        labels: BTreeMap::new(),
    };

    provider.pull(&manifest).await.unwrap();
    let created = provider.inspect("demo-service").await.unwrap();
    assert_eq!(created.status.phase, ServicePhase::Stopped);

    provider.start("demo-service").await.unwrap();
    sleep(Duration::from_millis(150)).await;

    let running = provider.inspect("demo-service").await.unwrap();
    assert!(running.status.is_running());

    let mut logs = ServiceLogs {
        raw: Vec::new(),
        text: String::new(),
    };
    for _ in 0..10 {
        logs = provider
            .logs(
                "demo-service",
                &ServiceLogsOptions {
                    tail: Some("10".into()),
                },
            )
            .await
            .unwrap();
        if logs.text.contains("fake-launcher") {
            break;
        }
        sleep(Duration::from_millis(100)).await;
    }
    assert!(logs.text.contains("fake-launcher"));
    assert!(logs.text.contains("run"));
    assert!(!logs.text.contains("serve"));
    assert!(logs.text.contains("-Shttp"));
    assert!(logs.text.contains("-Stcp"));
    assert!(logs.text.contains("-Sinherit-network"));
    assert!(logs.text.contains("-Sallow-ip-name-lookup"));

    provider.stop("demo-service").await.unwrap();
    let stopped = provider.inspect("demo-service").await.unwrap();
    assert!(!stopped.status.is_running());

    provider.remove("demo-service").await.unwrap();
    assert!(provider.inspect("demo-service").await.is_err());
}

#[test]
fn wasmtime_tcp_entry_runs_command_with_network_permissions() {
    let temp_dir = TempDir::new().unwrap();
    let component = temp_dir.path().join("demo.wasm");
    let manifest = ServiceManifest {
        name: "tcp-service".into(),
        definition_id: None,
        runtime: RuntimeKind::Wasmtime,
        source: ServiceSource::WasmtimeFile {
            component: component.clone(),
        },
        expose: Some(ServiceExpose {
            transport: ServiceExposeTransport {
                kind: ServiceExposeTransportKind::Tcp,
            },
            usage: Some(ServiceExposeUsage {
                kind: ServiceExposeUsageKind::Raw,
                path: None,
            }),
            icon_url: None,
        }),
        env: BTreeMap::from([
            ("SFTP_FS_ROOT".into(), "appdata/files with spaces".into()),
            ("HOME".into(), "guest-home=value".into()),
        ]),
        mounts: Vec::new(),
        ports: vec![ServicePort {
            name: Some("socks5".into()),
            host_port: 18081,
            host_port_allocation: ServicePortAllocation::Fixed,
            service_port: 1080,
            protocol: ServicePortProtocol::Tcp,
        }],
        command: vec!["--listen".into(), "127.0.0.1:1080".into()],
        entrypoint: Vec::new(),
        working_dir: None,
        labels: BTreeMap::new(),
    };
    let state = WasmtimeServiceState {
        manifest,
        source_display: component.display().to_string(),
        staged_component_path: component,
        service_dir: temp_dir.path().join("service"),
        runtime_dir: temp_dir.path().join("runtime"),
        log_file_path: temp_dir.path().join("runtime.log"),
        child: None,
        last_exit_code: None,
    };

    let command = build_wasmtime_command(Path::new("/bin/fungi"), temp_dir.path(), &state).unwrap();
    let args = command
        .as_std()
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();

    assert!(args.iter().any(|arg| arg == "run"));
    assert!(!args.iter().any(|arg| arg == "serve"));
    assert!(args.iter().any(|arg| arg == "-Scli"));
    assert!(args.iter().any(|arg| arg == "-Shttp"));
    assert!(args.iter().any(|arg| arg == "-Stcp"));
    assert!(args.iter().any(|arg| arg == "-Sinherit-network"));
    assert!(args.iter().any(|arg| arg == "-Sallow-ip-name-lookup"));
    assert!(args.iter().any(|arg| arg == "--listen"));
    let component_index = args
        .iter()
        .position(|arg| arg == &state.staged_component_path.to_string_lossy())
        .unwrap();
    for expected in [
        "SFTP_FS_ROOT=appdata/files with spaces",
        "HOME=guest-home=value",
    ] {
        assert!(
            args[..component_index]
                .windows(2)
                .any(|pair| pair == ["--env", expected])
        );
    }
    // Guest configuration must not overwrite the launcher's own environment
    // (notably the Android-specific HOME set by build_wasmtime_command).
    assert!(!command.as_std().get_envs().any(|(name, value)| {
        name == "SFTP_FS_ROOT" || (name == "HOME" && value == Some("guest-home=value".as_ref()))
    }));
}

#[test]
fn wasmtime_command_android_home_override_matches_target_behavior() {
    let temp_dir = TempDir::new().unwrap();
    let component = temp_dir.path().join("demo.wasm");
    let manifest = ServiceManifest {
        name: "android-home".into(),
        definition_id: None,
        runtime: RuntimeKind::Wasmtime,
        source: ServiceSource::WasmtimeFile {
            component: component.clone(),
        },
        expose: None,
        env: BTreeMap::new(),
        mounts: Vec::new(),
        ports: Vec::new(),
        command: Vec::new(),
        entrypoint: Vec::new(),
        working_dir: None,
        labels: BTreeMap::new(),
    };
    let state = WasmtimeServiceState {
        manifest,
        source_display: component.display().to_string(),
        staged_component_path: component,
        service_dir: temp_dir.path().join("service"),
        runtime_dir: temp_dir.path().join("runtime"),
        log_file_path: temp_dir.path().join("runtime.log"),
        child: None,
        last_exit_code: None,
    };

    let fungi_home = temp_dir.path().join(".fungi");
    let command = build_wasmtime_command(Path::new("/bin/fungi"), &fungi_home, &state).unwrap();
    let home_env = command
        .as_std()
        .get_envs()
        .find_map(|(key, value)| (key == "HOME").then_some(value))
        .flatten()
        .map(|value| value.to_string_lossy().into_owned());

    if cfg!(target_os = "android") {
        let expected = fungi_home.join("wasmtime");
        assert_eq!(
            home_env.as_deref(),
            Some(expected.to_string_lossy().as_ref())
        );
        assert!(expected.exists());
    } else {
        assert!(home_env.is_none());
    }
}

#[test]
fn wasmtime_command_without_tcp_ports_omits_network_permissions() {
    let temp_dir = TempDir::new().unwrap();
    let component = temp_dir.path().join("demo.wasm");
    let manifest = ServiceManifest {
        name: "http-service".into(),
        definition_id: None,
        runtime: RuntimeKind::Wasmtime,
        source: ServiceSource::WasmtimeFile {
            component: component.clone(),
        },
        expose: Some(ServiceExpose {
            transport: ServiceExposeTransport {
                kind: ServiceExposeTransportKind::Tcp,
            },
            usage: Some(ServiceExposeUsage {
                kind: ServiceExposeUsageKind::Web,
                path: Some("/".into()),
            }),
            icon_url: None,
        }),
        env: BTreeMap::new(),
        mounts: Vec::new(),
        ports: Vec::new(),
        command: Vec::new(),
        entrypoint: Vec::new(),
        working_dir: None,
        labels: BTreeMap::new(),
    };
    let state = WasmtimeServiceState {
        manifest,
        source_display: component.display().to_string(),
        staged_component_path: component,
        service_dir: temp_dir.path().join("service"),
        runtime_dir: temp_dir.path().join("runtime"),
        log_file_path: temp_dir.path().join("runtime.log"),
        child: None,
        last_exit_code: None,
    };

    let command = build_wasmtime_command(Path::new("/bin/fungi"), temp_dir.path(), &state).unwrap();
    let args = command
        .as_std()
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();

    assert!(args.iter().any(|arg| arg == "run"));
    assert!(!args.iter().any(|arg| arg == "serve"));
    assert!(args.iter().any(|arg| arg == "-Shttp"));
    assert!(!args.iter().any(|arg| arg == "-Stcp"));
    assert!(!args.iter().any(|arg| arg == "-Sinherit-network"));
    assert!(!args.iter().any(|arg| arg == "-Sallow-ip-name-lookup"));
}

#[test]
fn fungi_service_document_supports_fungi_workspace_and_auto_host_port() {
    let yaml = r#"
fungi: service/v1
id: filebrowser
run:
  provider: docker
  source:
    image: filebrowser/filebrowser:latest
  mounts:
    - from: $fungi.workspace
      to: /srv
publish:
  http:
    tcp:
      port: 80
    client:
      kind: web
"#;

    let occupied_allowed_port = StdTcpListener::bind(("127.0.0.1", 0)).unwrap();
    let occupied_allowed_port_number = occupied_allowed_port.local_addr().unwrap().port();
    let used_host_ports = BTreeSet::from([occupied_allowed_port_number]);
    let fungi_home = PathBuf::from("/tmp/fungi-home");
    let paths = FungiPaths::from_fungi_home(&fungi_home);
    let manifest = parse_service_manifest_yaml_with_policy(
        yaml,
        Path::new("."),
        &fungi_home,
        &ManifestResolutionPolicy,
        &used_host_ports,
    )
    .unwrap();

    assert_eq!(manifest.mounts[0].host_path, paths.user_home());
    assert_ne!(manifest.ports[0].host_port, occupied_allowed_port_number);
    assert_eq!(
        manifest.ports[0].host_port_allocation,
        ServicePortAllocation::Auto
    );
}

#[test]
fn fungi_service_document_supports_explicit_service_path_roots() {
    let yaml = r#"
fungi: service/v1
id: filebrowser
run:
  provider: docker
  source:
    image: filebrowser/filebrowser:latest
  mounts:
    - from: $fungi.service.data/db
      to: /srv
    - from: $fungi.service.artifacts/static
      to: /static
    - from: $fungi.root
      to: /user
publish:
  http:
    tcp:
      port: 80
"#;

    let fungi_home = PathBuf::from("/tmp/fungi-home");
    let local_service_id = "svc_01hz7j7n3evh1q4j1a8g9c2d3e";
    let paths = FungiPaths::from_fungi_home(&fungi_home);
    let path_roots =
        super::manifest::ManifestPathRoots::for_local_service_id(&fungi_home, local_service_id);
    let manifest = parse_service_manifest_yaml_with_policy_for_service_paths(
        yaml,
        Path::new("."),
        &path_roots,
        &ManifestResolutionPolicy,
        &BTreeSet::new(),
    )
    .unwrap();

    assert_eq!(
        manifest.mounts[0].host_path,
        paths.service_appdata_dir(local_service_id).join("db")
    );
    assert_eq!(
        manifest.mounts[1].host_path,
        paths.service_artifacts_dir(local_service_id).join("static")
    );
    assert_eq!(manifest.mounts[2].host_path, paths.user_root());
}

#[test]
fn fungi_service_file_maps_docker_workload_port_and_workspace_mount() {
    let content = r#"---
fungi: service/v1
id: code-server
run:
  provider: docker
  source:
    image: ghcr.io/coder/code-server:4.117.0
  args:
    - --bind-addr
    - 0.0.0.0:8080
  mounts:
    - from: $fungi.workspace
      to: /home/coder/project
publish:
  http:
    tcp:
      port: 8080
    client:
      kind: web
      path: /
---

# code-server
"#;

    let fungi_home = PathBuf::from("/tmp/fungi-home");
    let paths = FungiPaths::from_fungi_home(&fungi_home);
    let manifest = parse_service_manifest_yaml(content, Path::new("."), &fungi_home).unwrap();

    assert_eq!(manifest.name, "code-server");
    assert_eq!(manifest.runtime, RuntimeKind::Docker);
    assert_eq!(manifest.mounts[0].host_path, paths.user_home());
    assert_eq!(manifest.ports[0].service_port, 8080);
    assert_eq!(
        manifest.ports[0].host_port_allocation,
        ServicePortAllocation::Auto
    );
    assert_eq!(
        manifest
            .expose
            .as_ref()
            .unwrap()
            .usage
            .as_ref()
            .unwrap()
            .kind,
        ServiceExposeUsageKind::Web
    );
}

#[test]
fn fungi_service_file_rejects_removed_wasmtime_http_mode() {
    let content = r#"
fungi: service/v1
id: filebrowser-lite
run:
  provider: wasmtime
  mode: http
  source:
    url: https://example.test/filebrowser.wasm
publish:
  http:
    tcp:
      port: 8082
    client:
      kind: web
      path: /
"#;

    let error = parse_service_manifest_yaml(content, Path::new("."), Path::new("/tmp/fungi-home"))
        .unwrap_err();

    assert!(error.to_string().contains("unknown field `mode`"));
}

#[test]
fn fungi_service_file_maps_wasmtime_listener_to_run_args() {
    let content = r#"
fungi: service/v1
id: filebrowser-lite
run:
  provider: wasmtime
  source:
    url: https://example.test/filebrowser.wasm
  args:
    - --listen
    - 127.0.0.1:8082
publish:
  http:
    tcp:
      port: 8082
    client:
      kind: web
      path: /
"#;

    let manifest =
        parse_service_manifest_yaml(content, Path::new("."), Path::new("/tmp/fungi-home")).unwrap();

    assert_eq!(manifest.runtime, RuntimeKind::Wasmtime);
    assert_eq!(
        manifest.command,
        ["--listen".to_string(), "127.0.0.1:8082".to_string()]
    );
    assert_eq!(manifest.ports[0].host_port, 8082);
    assert_eq!(
        manifest.ports[0].host_port_allocation,
        ServicePortAllocation::Fixed
    );
}

#[test]
fn fungi_service_file_without_run_maps_to_external_tcp_service() {
    let content = r#"
fungi: service/v1
id: ssh-tunnel
publish:
  ssh:
    tcp:
      host: 127.0.0.1
      port: 22
    client:
      kind: ssh
"#;

    let manifest =
        parse_service_manifest_yaml(content, Path::new("."), Path::new("/tmp/fungi-home")).unwrap();

    assert_eq!(manifest.runtime, RuntimeKind::External);
    assert!(matches!(
        manifest.source,
        ServiceSource::ExistingTcp { ref host, port } if host == "127.0.0.1" && port == 22
    ));
    assert_eq!(manifest.ports[0].host_port, 22);
    assert_eq!(
        manifest
            .expose
            .as_ref()
            .unwrap()
            .usage
            .as_ref()
            .unwrap()
            .kind,
        ServiceExposeUsageKind::Ssh
    );
}

#[test]
fn fungi_service_file_rejects_mixed_client_metadata() {
    let content = r#"
fungi: service/v1
id: mixed
run:
  provider: docker
  source:
    image: example/mixed:latest
publish:
  web:
    tcp:
      port: 8080
    client:
      kind: web
      path: /
  ssh:
    tcp:
      port: 22
    client:
      kind: ssh
"#;

    let error = parse_service_manifest_yaml(content, Path::new("."), Path::new("/tmp/fungi-home"))
        .expect_err("mixed client metadata should be rejected");

    assert!(error.to_string().contains("client metadata must match"));
}

#[test]
fn fungi_service_yaml_allows_yaml_document_start_without_front_matter_close() {
    let content = r#"---
fungi: service/v1
id: ssh-tunnel
publish:
  ssh:
    tcp:
      host: 127.0.0.1
      port: 22
    client:
      kind: ssh
"#;

    let manifest =
        parse_service_manifest_yaml(content, Path::new("."), Path::new("/tmp/fungi-home")).unwrap();

    assert_eq!(manifest.name, "ssh-tunnel");
    assert_eq!(manifest.runtime, RuntimeKind::External);
}

#[test]
fn fungi_service_yaml_parse_error_keeps_field_detail() {
    let content = r#"
fungi: service/v1
id: broken
publish:
  main:
    tcp: {}
"#;

    let error = parse_service_manifest_yaml(content, Path::new("."), Path::new("/tmp/fungi-home"))
        .expect_err("missing tcp.port should be reported");
    let message = error.to_string();

    assert!(message.contains("Failed to parse Fungi service YAML"));
    assert!(message.contains("missing field `port`"));
}

#[test]
fn fungi_service_yaml_rejects_legacy_name_field() {
    let content = r#"
fungi: service/v1
name: old-name
publish:
  main:
    tcp:
      port: 1080
    client:
      kind: raw
"#;

    let error = parse_service_manifest_yaml(content, Path::new("."), Path::new("/tmp/fungi-home"))
        .expect_err("Fungi service files should require id instead of name");
    let message = error.to_string();

    assert!(message.contains("Failed to parse Fungi service YAML"));
    assert!(message.contains("unknown field `name`"));
}

#[test]
fn service_manifest_with_instance_name_keeps_front_matter_parseable() {
    let content = r#"---
fungi: service/v1
id: docs
publish:
  main:
    tcp:
      port: 8080
    client:
      kind: raw
---

# Docs
"#;

    let rendered = service_manifest_with_instance_name(content, "published-docs").unwrap();

    assert!(!rendered.starts_with("---\n---\n"));
    assert!(rendered.contains("instance: published-docs"));

    let manifest =
        parse_service_manifest_yaml(&rendered, Path::new("."), Path::new("/tmp/fungi-home"))
            .unwrap();

    assert_eq!(manifest.name, "published-docs");
    assert_eq!(manifest.definition_id.as_deref(), Some("docs"));
    assert_eq!(manifest.runtime, RuntimeKind::External);
}

#[test]
fn fungi_service_docker_publish_allocates_host_port() {
    let yaml = r#"
fungi: service/v1
id: filebrowser
run:
  provider: docker
  source:
    image: filebrowser/filebrowser:latest
publish:
  http:
    tcp:
      port: 80
"#;

    let manifest = parse_service_manifest_yaml(yaml, Path::new("/tmp"), Path::new("/tmp")).unwrap();

    assert!(manifest.ports[0].host_port > 0);
    assert_eq!(
        manifest.ports[0].host_port_allocation,
        ServicePortAllocation::Auto
    );
}

#[test]
fn service_manifest_to_yaml_renders_current_fungi_service_format() {
    let yaml = r#"
fungi: service/v1
id: code-server
run:
  provider: docker
  source:
    image: ghcr.io/coder/code-server:4.117.0
publish:
  http:
    tcp:
      port: 8080
    client:
      kind: web
"#;

    let manifest =
        parse_service_manifest_yaml(yaml, Path::new("."), Path::new("/tmp/fungi-home")).unwrap();
    assert_eq!(
        manifest.ports[0].host_port_allocation,
        ServicePortAllocation::Auto
    );

    let rendered = service_manifest_to_yaml(&manifest).unwrap();
    assert!(rendered.contains("fungi: service/v1"));
    assert!(rendered.contains("id: code-server"));
    assert!(rendered.contains("publish:"));
    assert!(rendered.contains("port: 8080"));

    let reparsed = parse_managed_service_manifest_yaml(
        &rendered,
        Path::new("."),
        Path::new("/tmp/fungi-home"),
        "svc_code_server",
    )
    .unwrap();
    assert_eq!(reparsed.ports[0].service_port, 8080);
    assert_eq!(
        reparsed.ports[0].host_port_allocation,
        ServicePortAllocation::Auto
    );
}

#[test]
fn service_manifest_to_yaml_preserves_fixed_wasmtime_publish_port() {
    let yaml = r#"
fungi: service/v1
id: fixed-web
run:
  provider: wasmtime
  source:
    url: https://example.test/fixed-web.wasm
publish:
  http:
    tcp:
      port: 8080
    client:
      kind: web
"#;

    let manifest =
        parse_service_manifest_yaml(yaml, Path::new("."), Path::new("/tmp/fungi-home")).unwrap();
    assert_eq!(manifest.ports[0].service_port, 8080);
    assert_eq!(manifest.ports[0].host_port, 8080);
    assert_eq!(
        manifest.ports[0].host_port_allocation,
        ServicePortAllocation::Fixed
    );

    let rendered = service_manifest_to_yaml(&manifest).unwrap();
    assert!(rendered.contains("fungi: service/v1"));
    assert!(rendered.contains("port: 8080"));

    let reparsed = parse_managed_service_manifest_yaml(
        &rendered,
        Path::new("."),
        Path::new("/tmp/fungi-home"),
        "svc_fixed_web",
    )
    .unwrap();
    assert_eq!(reparsed.ports[0].host_port, 8080);
    assert_eq!(
        reparsed.ports[0].host_port_allocation,
        ServicePortAllocation::Fixed
    );
}

#[test]
fn fungi_service_rejects_duplicate_fixed_publish_ports() {
    let yaml = r#"
fungi: service/v1
id: duplicate-host-port
run:
  provider: wasmtime
  source:
    url: https://example.test/web.wasm
publish:
  http:
    tcp:
      port: 18080
    client:
      kind: web
  metrics:
    tcp:
      port: 18080
"#;

    let error = parse_service_manifest_yaml(yaml, Path::new("."), Path::new("/tmp/fungi-home"))
        .expect_err("duplicate fixed publish ports should fail validation");
    assert!(
        error
            .to_string()
            .contains("publish.metrics.tcp.port 18080 is already reserved")
    );
}

#[test]
fn fungi_service_document_supports_external_tcp_service() {
    let yaml = r#"
fungi: service/v1
id: home-ssh
publish:
  ssh:
    tcp:
      host: 127.0.0.1
      port: 22
    client:
      kind: ssh
"#;

    let manifest =
        parse_service_manifest_yaml(yaml, Path::new("."), Path::new("/tmp/fungi-home")).unwrap();

    assert_eq!(manifest.runtime, RuntimeKind::External);
    assert!(matches!(
        manifest.source,
        ServiceSource::ExistingTcp { ref host, port } if host == "127.0.0.1" && port == 22
    ));
    assert_eq!(manifest.ports[0].name.as_deref(), Some("ssh"));
}

#[tokio::test]
async fn wasmtime_provider_downloads_remote_component() {
    let temp_dir = TempDir::new().unwrap();
    let launcher = create_fake_launcher(temp_dir.path()).unwrap();
    let server = spawn_http_server(b"downloaded-wasm".to_vec()).await;

    let provider = WasmtimeRuntimeProvider::new(
        temp_dir.path().join("runtime"),
        launcher,
        temp_dir.path().to_path_buf(),
        vec![temp_dir.path().to_path_buf()],
    );
    let manifest = ServiceManifest {
        name: "download-service".into(),
        definition_id: None,
        runtime: RuntimeKind::Wasmtime,
        source: ServiceSource::WasmtimeUrl {
            url: server.url.clone(),
        },
        expose: None,
        env: BTreeMap::new(),
        mounts: Vec::new(),
        ports: Vec::new(),
        command: vec!["--help".into()],
        entrypoint: Vec::new(),
        working_dir: None,
        labels: BTreeMap::new(),
    };

    let pulled = provider
        .pull_with_local_service_id(&manifest, "svc_download")
        .await
        .unwrap();
    assert_eq!(pulled.status.phase, ServicePhase::Stopped);
    assert!(
        temp_dir
            .path()
            .join("artifacts/services/svc_download/component.wasm")
            .exists()
    );
    drop(server);
}

#[tokio::test]
async fn runtime_control_apply_reuses_local_id_and_restages_wasmtime_component() {
    let temp_dir = TempDir::new().unwrap();
    let fungi_home = temp_dir.path().join("fungi-home");
    let component_v1 = temp_dir.path().join("component-v1.wasm");
    let component_v2 = temp_dir.path().join("component-v2.wasm");
    fs::write(&component_v1, b"wasm-v1").unwrap();
    fs::write(&component_v2, b"wasm-v2").unwrap();
    let launcher = create_fake_launcher(temp_dir.path()).unwrap();

    let control = RuntimeControl::new(
        fungi_home.join("runtime"),
        launcher,
        fungi_home.clone(),
        None,
        fungi_home.join("services"),
        vec![temp_dir.path().to_path_buf()],
        true,
    )
    .unwrap();

    let manifest_v1 = format!(
        r#"
fungi: service/v1
id: demo
run:
  provider: wasmtime
  source:
    file: {}
publish:
  main:
    tcp:
      port: 8080
"#,
        component_v1.display()
    );
    let applied_v1 = control
        .apply_manifest_yaml(
            &manifest_v1,
            temp_dir.path(),
            &fungi_home,
            &ManifestResolutionPolicy,
        )
        .await
        .unwrap();
    assert_eq!(applied_v1.instance.name, "demo");

    let local_service_id = fs::read_dir(fungi_home.join("services"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .file_name()
        .to_string_lossy()
        .to_string();
    let staged_component = fungi_home
        .join("artifacts/services")
        .join(&local_service_id)
        .join("component.wasm");
    assert_eq!(fs::read(&staged_component).unwrap(), b"wasm-v1");

    control.start_by_name("demo").await.unwrap();

    let manifest_v2 = format!(
        r#"
fungi: service/v1
id: demo
run:
  provider: wasmtime
  source:
    file: {}
publish:
  main:
    tcp:
      port: 8080
"#,
        component_v2.display()
    );
    let applied_v2 = control
        .apply_manifest_yaml(
            &manifest_v2,
            temp_dir.path(),
            &fungi_home,
            &ManifestResolutionPolicy,
        )
        .await
        .unwrap();

    assert!(applied_v2.previous_manifest.is_some());
    assert_eq!(
        fs::read_dir(fungi_home.join("services"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .file_name()
            .to_string_lossy(),
        local_service_id
    );
    assert_eq!(fs::read(&staged_component).unwrap(), b"wasm-v2");
    assert!(applied_v2.instance.status.is_running());
}

#[tokio::test]
async fn first_apply_persistence_failure_can_be_retried_without_restart() {
    let temp = TempDir::new().unwrap();
    let home = temp.path();
    let component = home.join("component.wasm");
    fs::write(&component, b"component").unwrap();
    let manifest = parse_service_manifest_yaml(&format!(
        "fungi: service/v1\nid: retryable\nrun:\n  provider: wasmtime\n  source:\n    file: {}\npublish:\n  main:\n    tcp:\n      port: 8082\n", component.display()
    ), home, home).unwrap();
    let provider = WasmtimeRuntimeProvider::new(
        home.join("runtime"),
        create_fake_launcher(home).unwrap(),
        home.to_path_buf(),
        vec![home.to_path_buf()],
    );
    let services = home.join("services");
    let control =
        RuntimeControl::with_wasmtime_provider(provider.clone(), None, services.clone(), true)
            .unwrap();

    // A non-directory obstruction fails identically for root and non-root test runners.
    fs::remove_dir(&services).unwrap();
    fs::write(&services, b"blocked").unwrap();
    let error = control.apply(&manifest).await.unwrap_err();
    assert!(format!("{error:#}").contains("Failed to create managed service directory"));
    assert!(!provider.has_service("retryable"));
    assert!(control.list_services().await.unwrap().is_empty());
    assert_eq!(
        fs::read_dir(home.join("artifacts/services"))
            .unwrap()
            .count(),
        0
    );

    fs::remove_file(&services).unwrap();
    fs::create_dir(&services).unwrap();
    control.apply(&manifest).await.unwrap();
    control.start_by_name("retryable").await.unwrap();
    assert!(
        control
            .inspect_by_name("retryable")
            .await
            .unwrap()
            .status
            .is_running()
    );
    control.remove_by_name("retryable").await.unwrap();
    assert!(!provider.has_service("retryable"));
}

#[tokio::test]
async fn failed_upgrade_restores_manifest_and_remains_retryable() {
    let temp = TempDir::new().unwrap();
    let home = temp.path();
    let component = home.join("component.wasm");
    fs::write(&component, b"upgraded").unwrap();
    let yaml = format!(
        "fungi: service/v1\nid: demo\nrun:\n  provider: wasmtime\n  source:\n    file: {}\npublish:\n  main:\n    tcp:\n      port: 8082\n",
        component.display()
    );
    let legacy = yaml.replace("  provider: wasmtime", "  provider: wasmtime\n  mode: http");
    let saved = home.join("services/svc_old");
    fs::create_dir_all(&saved).unwrap();
    fs::write(saved.join("service.yaml"), &legacy).unwrap();
    let old_state =
        br#"{"schema_version":2,"local_service_id":"svc_old","desired_state":"running"}"#;
    fs::write(saved.join("state.json"), old_state).unwrap();
    let data = home.join("appdata/services/svc_old");
    fs::create_dir_all(&data).unwrap();
    fs::write(data.join("keep.txt"), b"user data").unwrap();
    let provider = WasmtimeRuntimeProvider::new(
        home.join("runtime"),
        create_fake_launcher(home).unwrap(),
        home.to_path_buf(),
        vec![home.to_path_buf()],
    );
    let control =
        RuntimeControl::with_wasmtime_provider(provider.clone(), None, home.join("services"), true)
            .unwrap();
    control.restore_persisted_state().await.unwrap();

    fs::rename(saved.join("state.json"), saved.join("state.backup")).unwrap();
    fs::create_dir(saved.join("state.json")).unwrap();
    let error = control
        .apply_manifest_yaml(&yaml, home, home, &ManifestResolutionPolicy)
        .await
        .unwrap_err();
    assert!(format!("{error:#}").contains("Failed to persist service state file"));
    assert_eq!(
        fs::read_to_string(saved.join("service.yaml")).unwrap(),
        legacy
    );
    assert!(!provider.has_service("demo"));
    assert!(
        control
            .inspect_by_name("demo")
            .await
            .unwrap()
            .status
            .state_label()
            .contains("configuration error")
    );
    assert!(control.start_by_name("demo").await.is_err());
    assert_eq!(fs::read(data.join("keep.txt")).unwrap(), b"user data");

    fs::remove_dir(saved.join("state.json")).unwrap();
    fs::rename(saved.join("state.backup"), saved.join("state.json")).unwrap();
    assert_eq!(fs::read(saved.join("state.json")).unwrap(), old_state);
    control
        .apply_manifest_yaml(&yaml, home, home, &ManifestResolutionPolicy)
        .await
        .unwrap();
    control.start_by_name("demo").await.unwrap();
    assert!(
        control
            .inspect_by_name("demo")
            .await
            .unwrap()
            .status
            .is_running()
    );
    control.remove_by_name("demo").await.unwrap();
    assert_eq!(fs::read(data.join("keep.txt")).unwrap(), b"user data");
}

#[tokio::test]
async fn invalid_persisted_services_are_isolated_and_can_be_upgraded_or_removed() {
    let temp = TempDir::new().unwrap();
    let home = temp.path().join("home");
    let component = temp.path().join("upgraded.wasm");
    fs::write(&component, b"upgraded component").unwrap();
    let manifest = format!(
        "fungi: service/v1\nid: filebrowser-lite\ninstance: files\nrun:\n  provider: wasmtime\n  source:\n    file: {}\n  mounts:\n    - from: $fungi.service.data\n      to: /data\npublish:\n  http:\n    tcp:\n      port: 8082\n",
        component.display()
    );
    let legacy = manifest.replace("  provider: wasmtime", "  provider: wasmtime\n  mode: http");
    let service_dir = home.join("services/svc_old");
    fs::create_dir_all(&service_dir).unwrap();
    fs::write(service_dir.join("service.yaml"), &legacy).unwrap();
    fs::write(
        service_dir.join("state.json"),
        r#"{"schema_version":2,"local_service_id":"svc_old","desired_state":"running"}"#,
    )
    .unwrap();
    let data = home.join("appdata/services/svc_old");
    fs::create_dir_all(&data).unwrap();
    fs::write(data.join("keep.txt"), "user data").unwrap();
    let broken_dir = home.join("services/svc_broken");
    fs::create_dir_all(&broken_dir).unwrap();
    fs::write(broken_dir.join("service.yaml"), "fungi: [invalid YAML").unwrap();

    let control = RuntimeControl::new(
        home.join("runtime"),
        create_fake_launcher(temp.path()).unwrap(),
        home.clone(),
        None,
        home.join("services"),
        vec![temp.path().to_path_buf()],
        true,
    )
    .unwrap();
    control.restore_persisted_state().await.unwrap();
    assert!(control.desired_running_service_manifests().is_empty());
    let listed = control.list_services().await.unwrap();
    assert_eq!(listed.len(), 2);
    let failed = control.inspect_by_name("files").await.unwrap();
    assert_eq!(failed.status.phase, ServicePhase::Unknown);
    assert_eq!(failed.definition_id.as_deref(), Some("filebrowser-lite"));
    assert!(failed.status.detail.unwrap().contains("run-compatible"));
    assert!(
        control
            .start_by_name("files")
            .await
            .unwrap_err()
            .to_string()
            .contains("configuration error")
    );
    assert!(control.stop_by_name("files").await.is_err());
    assert_eq!(
        fs::read_to_string(service_dir.join("service.yaml")).unwrap(),
        legacy
    );
    assert!(
        control
            .list_published_device_services()
            .await
            .unwrap()
            .is_empty()
    );

    let mismatch = manifest.replace("id: filebrowser-lite", "id: different");
    assert!(
        control
            .apply_manifest_yaml(&mismatch, temp.path(), &home, &ManifestResolutionPolicy)
            .await
            .unwrap_err()
            .to_string()
            .contains("definition id")
    );
    assert!(
        control
            .apply_manifest_yaml(&legacy, temp.path(), &home, &ManifestResolutionPolicy)
            .await
            .is_err()
    );

    let applied = control
        .apply_manifest_yaml(&manifest, temp.path(), &home, &ManifestResolutionPolicy)
        .await
        .unwrap();
    assert_eq!(applied.instance.status.phase, ServicePhase::Stopped);
    assert_eq!(
        fs::read_to_string(data.join("keep.txt")).unwrap(),
        "user data"
    );
    assert_eq!(
        fs::read(home.join("artifacts/services/svc_old/component.wasm")).unwrap(),
        b"upgraded component"
    );
    let saved = fs::read_to_string(service_dir.join("service.yaml")).unwrap();
    assert!(!saved.contains("mode:"));
    assert_eq!(
        control.get_service_manifest("files").unwrap().mounts[0].host_path,
        data
    );
    control.start_by_name("files").await.unwrap();
    assert!(
        control
            .inspect_by_name("files")
            .await
            .unwrap()
            .status
            .is_running()
    );
    control.stop_by_name("files").await.unwrap();
    control.remove_by_name("svc_broken").await.unwrap();
    assert!(!broken_dir.exists());
    assert_eq!(control.list_services().await.unwrap().len(), 1);

    let reloaded = crate::service_state::ServiceStateStore::load(home.join("services")).unwrap();
    assert!(reloaded.failed_services().is_empty());
    assert_eq!(reloaded.local_service_id("files").unwrap(), "svc_old");
}

#[tokio::test]
async fn state_errors_do_not_block_healthy_services_and_apply_clears_migration_error() {
    let temp = TempDir::new().unwrap();
    let home = temp.path();
    let manifest = "fungi: service/v1\nid: demo\npublish:\n  main:\n    tcp:\n      port: 54321\n";
    for (id, name, state) in [
        (
            "svc_migrated",
            "migrated",
            r#"{"schema_version":2,"local_service_id":"svc_migrated","desired_state":"running","configuration_error":"Legacy Wasmtime HTTP service requires a run-compatible component"}"#,
        ),
        (
            "svc_future",
            "future",
            r#"{"schema_version":999,"local_service_id":"svc_future","desired_state":"running"}"#,
        ),
        ("svc_bad_state", "bad-state", "not json"),
        (
            "svc_healthy",
            "healthy",
            r#"{"schema_version":2,"local_service_id":"svc_healthy","desired_state":"running"}"#,
        ),
    ] {
        let dir = home.join("services").join(id);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("service.yaml"),
            manifest.replace("id: demo", &format!("id: {name}")),
        )
        .unwrap();
        fs::write(dir.join("state.json"), state).unwrap();
    }
    let control = RuntimeControl::new(
        home.join("runtime"),
        PathBuf::from("unused"),
        home.to_path_buf(),
        None,
        home.join("services"),
        vec![],
        false,
    )
    .unwrap();
    control.restore_persisted_state().await.unwrap();
    assert_eq!(control.list_services().await.unwrap().len(), 4);
    assert!(
        control
            .inspect_by_name("healthy")
            .await
            .unwrap()
            .status
            .is_running()
    );
    for name in ["migrated", "future", "bad-state"] {
        assert_eq!(
            control.inspect_by_name(name).await.unwrap().status.phase,
            ServicePhase::Unknown
        );
        assert!(
            control
                .start_by_name(name)
                .await
                .unwrap_err()
                .to_string()
                .contains("configuration error")
        );
    }
    let updated = manifest
        .replace("id: demo", "id: migrated")
        .replace("54321", "54322");
    control
        .apply_manifest_yaml(&updated, home, home, &ManifestResolutionPolicy)
        .await
        .unwrap();
    let state = fs::read_to_string(home.join("services/svc_migrated/state.json")).unwrap();
    assert!(!state.contains("configuration_error"));
    assert!(
        crate::service_state::ServiceStateStore::load(home.join("services"))
            .unwrap()
            .persisted_service("migrated")
            .is_some()
    );
}

#[tokio::test]
async fn runtime_control_apply_uses_in_memory_manifest_when_persisted_state_is_missing() {
    let temp_dir = TempDir::new().unwrap();
    let fungi_home = temp_dir.path().join("fungi-home");
    let control = RuntimeControl::new(
        fungi_home.join("runtime"),
        PathBuf::from("/bin/echo"),
        fungi_home.clone(),
        None,
        fungi_home.join("services"),
        Vec::new(),
        false,
    )
    .unwrap();

    let previous_manifest = existing_tcp_manifest("demo", "127.0.0.1", 22);
    control.seed_in_memory_service_for_test(previous_manifest);

    let applied = control
        .apply(&existing_tcp_manifest("demo", "127.0.0.1", 23))
        .await
        .unwrap();

    assert!(matches!(
        applied.previous_manifest.unwrap().source,
        ServiceSource::ExistingTcp { ref host, port } if host == "127.0.0.1" && port == 22
    ));
    assert_eq!(applied.desired_state, DesiredServiceState::Stopped);
    assert_eq!(applied.instance.source, "127.0.0.1:23");
}

#[tokio::test]
async fn apply_manifest_yaml_allows_same_service_fixed_host_port_reapply_only() {
    let temp_dir = TempDir::new().unwrap();
    let fungi_home = temp_dir.path().join("fungi-home");
    let component = temp_dir.path().join("component.wasm");
    fs::write(&component, b"wasm").unwrap();
    let launcher = create_fake_launcher(temp_dir.path()).unwrap();

    let control = RuntimeControl::new(
        fungi_home.join("runtime"),
        launcher,
        fungi_home.clone(),
        None,
        fungi_home.join("services"),
        vec![temp_dir.path().to_path_buf()],
        true,
    )
    .unwrap();

    let demo_manifest = wasmtime_manifest_yaml("demo", &component, 19100);
    control
        .apply_manifest_yaml(
            &demo_manifest,
            temp_dir.path(),
            &fungi_home,
            &ManifestResolutionPolicy,
        )
        .await
        .unwrap();
    control
        .apply_manifest_yaml(
            &demo_manifest,
            temp_dir.path(),
            &fungi_home,
            &ManifestResolutionPolicy,
        )
        .await
        .unwrap();

    let error = control
        .apply_manifest_yaml(
            &wasmtime_manifest_yaml("other", &component, 19100),
            temp_dir.path(),
            &fungi_home,
            &ManifestResolutionPolicy,
        )
        .await
        .expect_err("different service should not reuse a fixed publish port");

    assert!(
        error
            .to_string()
            .contains("publish.main.tcp.port 19100 is already reserved")
    );
}

#[tokio::test]
async fn apply_manifest_yaml_rejects_definition_id_mismatch() {
    let temp_dir = TempDir::new().unwrap();
    let fungi_home = temp_dir.path().join("fungi-home");
    let control = RuntimeControl::new(
        fungi_home.join("runtime"),
        PathBuf::from("/bin/echo"),
        fungi_home.clone(),
        None,
        fungi_home.join("services"),
        Vec::new(),
        false,
    )
    .unwrap();

    let code_server = r#"
fungi: service/v1
id: code-server
publish:
  main:
    tcp:
      host: 127.0.0.1
      port: 18080
    client:
      kind: raw
"#;
    control
        .apply_manifest_yaml(
            code_server,
            temp_dir.path(),
            &fungi_home,
            &ManifestResolutionPolicy,
        )
        .await
        .unwrap();

    let filebrowser_as_code_server = r#"
fungi: service/v1
id: filebrowser-lite
instance: code-server
publish:
  main:
    tcp:
      host: 127.0.0.1
      port: 18081
    client:
      kind: raw
"#;
    let error = control
        .apply_manifest_yaml(
            filebrowser_as_code_server,
            temp_dir.path(),
            &fungi_home,
            &ManifestResolutionPolicy,
        )
        .await
        .expect_err("different definition ids should not replace an existing service");

    let message = error.to_string();
    assert!(message.contains("definition id 'code-server'"));
    assert!(message.contains("definition id 'filebrowser-lite'"));
}

#[test]
fn parse_fungi_service_expose_defaults_service_identity() {
    let yaml = r#"
fungi: service/v1
id: filebrowser
run:
  provider: docker
  source:
    image: filebrowser/filebrowser:latest
publish:
  http:
    tcp:
      port: 80
    client:
      kind: web
      path: /
"#;

    let manifest = parse_service_manifest_yaml(yaml, Path::new("/tmp"), Path::new("/tmp")).unwrap();
    let expose = manifest.expose.as_ref().expect("expected expose config");
    assert_eq!(expose.transport.kind, ServiceExposeTransportKind::Tcp);
    let usage = expose.usage.as_ref().expect("expected usage config");
    assert_eq!(usage.kind, ServiceExposeUsageKind::Web);
    assert_eq!(usage.path.as_deref(), Some("/"));
}

#[test]
fn parse_fungi_service_expose_maps_icon_url() {
    let yaml = r#"
fungi: service/v1
id: filebrowser
run:
  provider: docker
  source:
    image: filebrowser/filebrowser:latest
publish:
  http:
    tcp:
      port: 80
    client:
      kind: web
      path: /
      iconUrl: https://example.test/icon.svg
"#;

    let manifest = parse_service_manifest_yaml(yaml, Path::new("/tmp"), Path::new("/tmp")).unwrap();
    let expose = manifest.expose.as_ref().expect("expected expose config");

    assert_eq!(
        expose.icon_url.as_deref(),
        Some("https://example.test/icon.svg")
    );
    let rendered = service_manifest_to_yaml(&manifest).unwrap();
    assert!(rendered.contains("iconUrl: https://example.test/icon.svg"));
}

#[test]
fn parse_fungi_service_rejects_mismatched_multi_entry_client_metadata() {
    let yaml = r#"
fungi: service/v1
id: multi
run:
  provider: docker
  source:
    image: example/multi:latest
publish:
  api:
    tcp:
      port: 8081
    client:
      kind: raw
  web:
    tcp:
      port: 8080
    client:
      kind: web
      path: /
"#;

    let error = parse_service_manifest_yaml(yaml, Path::new("/tmp"), Path::new("/tmp"))
        .expect_err("per-entry client metadata is not supported yet");
    assert!(error.to_string().contains("client metadata must match"));
}

#[test]
fn parse_fungi_service_rejects_docker_publish_host() {
    let yaml = r#"
fungi: service/v1
id: web-service
run:
  provider: docker
  source:
    image: example/web:latest
publish:
  main:
    tcp:
      host: 127.0.0.1
      port: 1234
"#;

    let error = parse_service_manifest_yaml(yaml, Path::new("/tmp"), Path::new("/tmp"))
        .expect_err("docker publish host should be rejected");
    assert!(
        error
            .to_string()
            .contains("publish.main.tcp.host is not used with provider: docker")
    );
}

#[test]
fn parse_fungi_service_rejects_zero_publish_port() {
    let yaml = r#"
fungi: service/v1
id: raw-service
publish:
  main:
    tcp:
      host: 127.0.0.1
      port: 0
"#;

    let error = parse_service_manifest_yaml(yaml, Path::new("/tmp"), Path::new("/tmp"))
        .expect_err("zero publish port should be rejected");
    assert!(
        error
            .to_string()
            .contains("publish.main.tcp.port must be greater than 0")
    );
}

#[test]
fn missing_docker_container_error_is_detected() {
    let error = anyhow::Error::new(DockerAgentError::DockerApi {
        status: "404".parse().unwrap(),
        message: "No such container: filebrowser".into(),
    });

    assert!(is_missing_docker_container_error(&error));
}

#[test]
fn non_404_docker_error_is_not_detected_as_missing_container() {
    let error = anyhow::Error::new(DockerAgentError::DockerApi {
        status: "500".parse().unwrap(),
        message: "daemon broke".into(),
    });

    assert!(!is_missing_docker_container_error(&error));
}

fn create_fake_launcher(dir: &Path) -> Result<PathBuf> {
    #[cfg(unix)]
    let (launcher, script) = (
        dir.join("fake-fungi.sh"),
        r#"#!/bin/sh
echo fake-launcher "$@"
sleep 30
"#,
    );
    #[cfg(windows)]
    let (launcher, script) = (
        dir.join("fake-fungi.cmd"),
        "@echo off\r\necho fake-launcher %*\r\nfor /L %%i in (1,1,100000000) do rem\r\n",
    );

    let mut file = fs::File::create(&launcher)?;
    file.write_all(script.as_bytes())?;
    #[cfg(unix)]
    {
        let mut permissions = fs::metadata(&launcher)?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&launcher, permissions)?;
    }
    Ok(launcher)
}

struct TestHttpServer {
    url: String,
}

async fn spawn_http_server(body: Vec<u8>) -> TestHttpServer {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buffer = [0_u8; 1024];
        let _ = socket.read(&mut buffer).await.unwrap();
        let mut response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .into_bytes();
        response.extend_from_slice(&body);
        socket.write_all(&response).await.unwrap();
    });

    TestHttpServer {
        url: format!("http://{addr}/app.wasm"),
    }
}

fn existing_tcp_manifest(name: &str, host: &str, port: u16) -> ServiceManifest {
    ServiceManifest {
        name: name.to_string(),
        definition_id: None,
        runtime: RuntimeKind::External,
        source: ServiceSource::ExistingTcp {
            host: host.to_string(),
            port,
        },
        expose: None,
        env: BTreeMap::new(),
        mounts: Vec::new(),
        ports: vec![ServicePort {
            name: Some("main".to_string()),
            host_port: port,
            host_port_allocation: ServicePortAllocation::Fixed,
            service_port: port,
            protocol: ServicePortProtocol::Tcp,
        }],
        command: Vec::new(),
        entrypoint: Vec::new(),
        working_dir: None,
        labels: BTreeMap::new(),
    }
}

fn wasmtime_manifest_yaml(name: &str, component: &Path, host_port: u16) -> String {
    format!(
        r#"
fungi: service/v1
id: {name}
run:
  provider: wasmtime
  source:
    file: {}
publish:
  main:
    tcp:
      port: {host_port}
"#,
        component.display()
    )
}
