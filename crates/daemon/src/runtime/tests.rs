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
    build_wasmtime_command, docker_spec_from_manifest, ensure_manifest_mount_dirs,
    is_missing_docker_container_error,
};
use super::providers::WasmtimeServiceState;

#[test]
fn docker_manifest_maps_to_container_spec() {
    let manifest = ServiceManifest {
        name: "filebrowser".into(),
        runtime: RuntimeKind::Docker,
        run_mode: ServiceRunMode::Command,
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

    let spec = docker_spec_from_manifest(&manifest).unwrap();
    assert_eq!(spec.name.as_deref(), Some("filebrowser"));
    assert_eq!(spec.image, "filebrowser/filebrowser:latest");
    assert_eq!(spec.ports[0].host_port, 8080);
}

#[test]
fn ensure_manifest_mount_dirs_creates_missing_host_paths() {
    let temp_dir = TempDir::new().unwrap();
    let mount_path = temp_dir.path().join("nested/data");
    let manifest = ServiceManifest {
        name: "mount-test".into(),
        runtime: RuntimeKind::Wasmtime,
        run_mode: ServiceRunMode::Command,
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
        runtime: RuntimeKind::Docker,
        run_mode: ServiceRunMode::Command,
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

    assert!(docker_spec_from_manifest(&manifest).is_err());
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
        runtime: RuntimeKind::Wasmtime,
        run_mode: ServiceRunMode::Http,
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
            catalog_id: None,
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
    assert_eq!(created.status.state, "created");

    provider.start("demo-service").await.unwrap();
    sleep(Duration::from_millis(150)).await;

    let running = provider.inspect("demo-service").await.unwrap();
    assert!(running.status.running);

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
    assert!(logs.text.contains("serve"));
    assert!(logs.text.contains("-Stcp"));
    assert!(logs.text.contains("-Sinherit-network"));
    assert!(logs.text.contains("-Sallow-ip-name-lookup"));

    provider.stop("demo-service").await.unwrap();
    let stopped = provider.inspect("demo-service").await.unwrap();
    assert!(!stopped.status.running);

    provider.remove("demo-service").await.unwrap();
    assert!(provider.inspect("demo-service").await.is_err());
}

#[test]
fn wasmtime_tcp_entry_runs_command_with_network_permissions() {
    let temp_dir = TempDir::new().unwrap();
    let component = temp_dir.path().join("demo.wasm");
    let manifest = ServiceManifest {
        name: "tcp-service".into(),
        runtime: RuntimeKind::Wasmtime,
        run_mode: ServiceRunMode::Command,
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
            catalog_id: None,
        }),
        env: BTreeMap::new(),
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
    assert!(args.iter().any(|arg| arg == "-Stcp"));
    assert!(args.iter().any(|arg| arg == "-Sinherit-network"));
    assert!(args.iter().any(|arg| arg == "-Sallow-ip-name-lookup"));
    assert!(args.iter().any(|arg| arg == "--listen"));
}

#[test]
fn wasmtime_http_mode_without_tcp_port_returns_error() {
    let temp_dir = TempDir::new().unwrap();
    let component = temp_dir.path().join("demo.wasm");
    let manifest = ServiceManifest {
        name: "http-service".into(),
        runtime: RuntimeKind::Wasmtime,
        run_mode: ServiceRunMode::Http,
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
            catalog_id: None,
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

    let error = build_wasmtime_command(Path::new("/bin/fungi"), temp_dir.path(), &state)
        .expect_err("http mode without a TCP port should be rejected");

    assert!(error.to_string().contains("requires at least one TCP port"));
}

#[test]
fn manifest_document_supports_user_home_and_auto_host_port() {
    let yaml = r#"
apiVersion: fungi.rs/v1alpha1
kind: Service
metadata:
    name: filebrowser
spec:
    run:
        docker:
            image: filebrowser/filebrowser:latest
    entries:
        http:
            port: 80
            usage: web
    mounts:
        - hostPath: ${USER_HOME}
          runtimePath: /srv
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
        &ManifestResolutionPolicy::default(),
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
fn manifest_document_supports_explicit_service_path_roots() {
    let yaml = r#"
apiVersion: fungi.rs/v1alpha1
kind: Service
metadata:
    name: filebrowser
spec:
    run:
        docker:
            image: filebrowser/filebrowser:latest
    entries:
        http:
            port: 80
    mounts:
        - hostPath: ${SERVICE_APPDATA}/db
          runtimePath: /srv
        - hostPath: ${SERVICE_ARTIFACTS}/static
          runtimePath: /static
        - hostPath: ${USER_ROOT}
          runtimePath: /user
    workingDir: ${USER_HOME}
"#;

    let fungi_home = PathBuf::from("/tmp/fungi-home");
    let local_service_id = "svc_01hz7j7n3evh1q4j1a8g9c2d3e";
    let paths = FungiPaths::from_fungi_home(&fungi_home);
    let path_roots =
        super::manifest::ManifestPathRoots::for_local_service_id(&fungi_home, local_service_id);
    let manifest = parse_service_manifest_yaml_with_policy_for_service_paths(
        yaml,
        Path::new("."),
        &fungi_home,
        &path_roots,
        &ManifestResolutionPolicy::default(),
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
    assert_eq!(
        manifest.working_dir,
        Some(paths.user_home().to_string_lossy().to_string())
    );
}

#[test]
fn fungi_service_file_maps_docker_workload_port_and_workspace_mount() {
    let content = r#"---
fungi: service/v1
name: code-server
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
    assert_eq!(manifest.run_mode, ServiceRunMode::Command);
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
fn fungi_service_file_maps_wasmtime_http_mode_to_serve_intent() {
    let content = r#"
fungi: service/v1
name: filebrowser-lite
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

    let manifest =
        parse_service_manifest_yaml(content, Path::new("."), Path::new("/tmp/fungi-home")).unwrap();

    assert_eq!(manifest.runtime, RuntimeKind::Wasmtime);
    assert_eq!(manifest.run_mode, ServiceRunMode::Http);
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
name: ssh-tunnel
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
name: mixed
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
name: ssh-tunnel
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
name: broken
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
fn manifest_document_defaults_missing_host_port_to_auto() {
    let yaml = r#"
apiVersion: fungi.rs/v1alpha1
kind: Service
metadata:
    name: filebrowser
spec:
    run:
        docker:
            image: filebrowser/filebrowser:latest
    entries:
        http:
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
fn service_manifest_to_yaml_preserves_resolved_auto_host_port() {
    let yaml = r#"
apiVersion: fungi.rs/v1alpha1
kind: Service
metadata:
    name: code-server
spec:
    run:
        docker:
            image: ghcr.io/coder/code-server:4.117.0
    entries:
        http:
            port: 8080
            usage: web
"#;

    let manifest =
        parse_service_manifest_yaml(yaml, Path::new("."), Path::new("/tmp/fungi-home")).unwrap();
    let resolved_host_port = manifest.ports[0].host_port;
    assert_eq!(
        manifest.ports[0].host_port_allocation,
        ServicePortAllocation::Auto
    );

    let rendered = service_manifest_to_yaml(&manifest).unwrap();
    assert!(rendered.contains(&format!("hostPort: {resolved_host_port}")));

    let reparsed = parse_managed_service_manifest_yaml(
        &rendered,
        Path::new("."),
        Path::new("/tmp/fungi-home"),
        "svc_code_server",
    )
    .unwrap();
    assert_eq!(reparsed.ports[0].host_port, resolved_host_port);
    assert_eq!(
        reparsed.ports[0].host_port_allocation,
        ServicePortAllocation::Fixed
    );
}

#[test]
fn service_manifest_to_yaml_preserves_fixed_host_port_equal_to_service_port() {
    let yaml = r#"
apiVersion: fungi.rs/v1alpha1
kind: Service
metadata:
    name: fixed-web
spec:
    run:
        docker:
            image: example/web:latest
    entries:
        http:
            port: 8080
            hostPort: 8080
            usage: web
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
    assert!(rendered.contains("hostPort: 8080"));

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
fn manifest_document_rejects_duplicate_explicit_host_ports() {
    let yaml = r#"
apiVersion: fungi.rs/v1alpha1
kind: Service
metadata:
    name: duplicate-host-port
spec:
    run:
        docker:
            image: example/web:latest
    entries:
        http:
            port: 8080
            hostPort: 18080
            usage: web
        metrics:
            port: 9090
            hostPort: 18080
"#;

    let error = parse_service_manifest_yaml(yaml, Path::new("."), Path::new("/tmp/fungi-home"))
        .expect_err("duplicate hostPort should fail validation");
    assert!(
        error
            .to_string()
            .contains("spec.entries.metrics.hostPort 18080 is already reserved")
    );
}

#[test]
fn manifest_document_supports_external_tcp_service() {
    let yaml = r#"
apiVersion: fungi.rs/v1alpha1
kind: Service
metadata:
    name: home-ssh
spec:
    entries:
        ssh:
            target: 127.0.0.1:22
            usage: ssh
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
        runtime: RuntimeKind::Wasmtime,
        run_mode: ServiceRunMode::Command,
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
    assert_eq!(pulled.status.state, "created");
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
apiVersion: fungi.rs/v1alpha1
kind: Service
metadata:
  name: demo
spec:
  run:
    wasmtime:
      file: {}
  entries:
    main:
      port: 8080
"#,
        component_v1.display()
    );
    let applied_v1 = control
        .apply_manifest_yaml(
            &manifest_v1,
            temp_dir.path(),
            &fungi_home,
            &ManifestResolutionPolicy::default(),
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
apiVersion: fungi.rs/v1alpha1
kind: Service
metadata:
  name: demo
spec:
  run:
    wasmtime:
      file: {}
  entries:
    main:
      port: 8080
"#,
        component_v2.display()
    );
    let applied_v2 = control
        .apply_manifest_yaml(
            &manifest_v2,
            temp_dir.path(),
            &fungi_home,
            &ManifestResolutionPolicy::default(),
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
    assert!(applied_v2.instance.status.running);
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
            &ManifestResolutionPolicy::default(),
        )
        .await
        .unwrap();
    control
        .apply_manifest_yaml(
            &demo_manifest,
            temp_dir.path(),
            &fungi_home,
            &ManifestResolutionPolicy::default(),
        )
        .await
        .unwrap();

    let error = control
        .apply_manifest_yaml(
            &wasmtime_manifest_yaml("other", &component, 19100),
            temp_dir.path(),
            &fungi_home,
            &ManifestResolutionPolicy::default(),
        )
        .await
        .err()
        .expect("different service should not reuse a fixed hostPort");

    assert!(
        error
            .to_string()
            .contains("spec.entries.main.hostPort 19100 is already reserved")
    );
}

#[test]
fn parse_manifest_expose_defaults_service_identity() {
    let yaml = r#"
apiVersion: fungi.rs/v1alpha1
kind: Service
metadata:
    name: filebrowser
spec:
    run:
        docker:
            image: filebrowser/filebrowser:latest
    entries:
        http:
            port: 80
            usage: web
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
fn parse_manifest_expose_maps_icon_url_and_catalog_id() {
    let yaml = r#"
apiVersion: fungi.rs/v1alpha1
kind: Service
metadata:
    name: filebrowser
spec:
    run:
        docker:
            image: filebrowser/filebrowser:latest
    entries:
        http:
            port: 80
            usage: web
            path: /
            iconUrl: https://example.test/icon.svg
            catalogId: io.example.filebrowser
"#;

    let manifest = parse_service_manifest_yaml(yaml, Path::new("/tmp"), Path::new("/tmp")).unwrap();
    let expose = manifest.expose.as_ref().expect("expected expose config");

    assert_eq!(
        expose.icon_url.as_deref(),
        Some("https://example.test/icon.svg")
    );
    assert_eq!(expose.catalog_id.as_deref(), Some("io.example.filebrowser"));

    let rendered = service_manifest_to_yaml(&manifest).unwrap();
    assert!(rendered.contains("iconUrl: https://example.test/icon.svg"));
    assert!(rendered.contains("catalogId: io.example.filebrowser"));
}

#[test]
fn parse_manifest_rejects_mismatched_multi_entry_expose_metadata() {
    let yaml = r#"
apiVersion: fungi.rs/v1alpha1
kind: Service
metadata:
    name: multi
spec:
    run:
        docker:
            image: example/multi:latest
    entries:
        api:
            port: 8081
            usage: tcp
        web:
            port: 8080
            usage: web
            path: /
"#;

    let error = parse_service_manifest_yaml(yaml, Path::new("/tmp"), Path::new("/tmp"))
        .expect_err("per-entry expose metadata is not supported yet");
    assert!(error.to_string().contains("expose metadata must match"));
}

#[test]
fn parse_manifest_rejects_entry_with_target_and_port() {
    let yaml = r#"
apiVersion: fungi.rs/v1alpha1
kind: Service
metadata:
    name: raw-service
spec:
    entries:
        main:
            target: 127.0.0.1:1234
            port: 1234
"#;

    let error = parse_service_manifest_yaml(yaml, Path::new("/tmp"), Path::new("/tmp"))
        .expect_err("entry cannot use target and port together");
    assert!(error.to_string().contains("either target or port"));
}

#[test]
fn parse_manifest_rejects_entry_without_target_or_port() {
    let yaml = r#"
apiVersion: fungi.rs/v1alpha1
kind: Service
metadata:
    name: raw-service
spec:
    entries:
        main:
            usage: tcp
"#;

    let error = parse_service_manifest_yaml(yaml, Path::new("/tmp"), Path::new("/tmp"))
        .expect_err("entry requires target or port");
    assert!(error.to_string().contains("requires target or port"));
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
        runtime: RuntimeKind::External,
        run_mode: ServiceRunMode::Command,
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
apiVersion: fungi.rs/v1alpha1
kind: Service
metadata:
  name: {name}
spec:
  run:
    wasmtime:
      file: {}
  entries:
    main:
      port: 8080
      hostPort: {host_port}
"#,
        component.display()
    )
}
