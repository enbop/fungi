use super::*;
use anyhow::Result;
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
    docker_spec_from_manifest, ensure_manifest_mount_dirs, is_missing_docker_container_error,
};

#[test]
fn docker_manifest_maps_to_container_spec() {
    let manifest = ServiceManifest {
        name: "filebrowser".into(),
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
    assert!(fungi_home.join("data").is_dir());
}

#[test]
fn docker_manifest_rejects_wrong_source_type() {
    let manifest = ServiceManifest {
        name: "bad".into(),
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
        source: ServiceSource::WasmtimeFile {
            component: component.clone(),
        },
        expose: None,
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

    provider.stop("demo-service").await.unwrap();
    let stopped = provider.inspect("demo-service").await.unwrap();
    assert!(!stopped.status.running);

    provider.remove("demo-service").await.unwrap();
    assert!(provider.inspect("demo-service").await.is_err());
}

#[test]
fn manifest_document_supports_app_home_and_auto_host_port() {
    let yaml = r#"
apiVersion: fungi.rs/v1alpha1
kind: ServiceManifest
metadata:
    name: filebrowser
spec:
    runtime: docker
    source:
        image: filebrowser/filebrowser:latest
    mounts:
        - hostPath: ${APP_HOME}/data
          runtimePath: /srv
    ports:
        - hostPort: auto
          servicePort: 80
          protocol: tcp
"#;

    let occupied_allowed_port = StdTcpListener::bind(("127.0.0.1", 0)).unwrap();
    let occupied_allowed_port_number = occupied_allowed_port.local_addr().unwrap().port();
    let used_host_ports = BTreeSet::from([occupied_allowed_port_number]);
    let fungi_home = PathBuf::from("/tmp/fungi-home");
    let manifest = parse_service_manifest_yaml_with_policy(
        yaml,
        Path::new("."),
        &fungi_home,
        &ManifestResolutionPolicy::default(),
        &used_host_ports,
    )
    .unwrap();

    assert_eq!(
        manifest.mounts[0].host_path,
        fungi_home.join("data/filebrowser/data")
    );
    assert_ne!(manifest.ports[0].host_port, occupied_allowed_port_number);
    assert_eq!(
        manifest.ports[0].host_port_allocation,
        ServicePortAllocation::Auto
    );
}

#[test]
fn manifest_document_supports_explicit_service_data_dir() {
    let yaml = r#"
apiVersion: fungi.rs/v1alpha1
kind: ServiceManifest
metadata:
    name: filebrowser
spec:
    runtime: docker
    source:
        image: filebrowser/filebrowser:latest
    mounts:
        - hostPath: ${APP_HOME}/data
          runtimePath: /srv
"#;

    let fungi_home = PathBuf::from("/tmp/fungi-home");
    let service_data_dir = fungi_home.join("data/svc_01hz7j7n3evh1q4j1a8g9c2d3e");
    let manifest = parse_service_manifest_yaml_with_policy_for_service_data_dir(
        yaml,
        Path::new("."),
        &fungi_home,
        &service_data_dir,
        &ManifestResolutionPolicy::default(),
        &BTreeSet::new(),
    )
    .unwrap();

    assert_eq!(manifest.mounts[0].host_path, service_data_dir.join("data"));
}

#[test]
fn manifest_document_defaults_missing_host_port_to_auto() {
    let yaml = r#"
apiVersion: fungi.rs/v1alpha1
kind: ServiceManifest
metadata:
    name: filebrowser
spec:
    runtime: docker
    source:
        image: filebrowser/filebrowser:latest
    ports:
        - servicePort: 80
          protocol: tcp
"#;

    let manifest = parse_service_manifest_yaml(yaml, Path::new("/tmp"), Path::new("/tmp")).unwrap();

    assert!(manifest.ports[0].host_port > 0);
    assert_eq!(
        manifest.ports[0].host_port_allocation,
        ServicePortAllocation::Auto
    );
}

#[test]
fn manifest_document_supports_link_service() {
    let yaml = r#"
apiVersion: fungi.rs/v1alpha1
kind: ServiceManifest
metadata:
    name: home-ssh
spec:
    runtime: link
    source:
        host: 127.0.0.1
        port: 22
    expose:
        enabled: true
        transport:
            kind: tcp
        usage:
            kind: ssh
    ports:
        - hostPort: 22
          servicePort: 22
          name: ssh
          protocol: tcp
"#;

    let manifest =
        parse_service_manifest_yaml(yaml, Path::new("."), Path::new("/tmp/fungi-home")).unwrap();

    assert_eq!(manifest.runtime, RuntimeKind::Link);
    assert!(matches!(
        manifest.source,
        ServiceSource::TcpLink { ref host, port } if host == "127.0.0.1" && port == 22
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

    let pulled = provider.pull(&manifest).await.unwrap();
    assert_eq!(pulled.status.state, "created");
    assert!(
        temp_dir
            .path()
            .join("runtime/wasmtime/download-service/component.wasm")
            .exists()
    );
    drop(server);
}

#[test]
fn parse_manifest_expose_defaults_service_identity() {
    let yaml = r#"
apiVersion: fungi.rs/v1alpha1
kind: ServiceManifest
metadata:
    name: filebrowser
spec:
    runtime: docker
    expose:
        enabled: true
        transport:
            kind: tcp
        usage:
            kind: web
            path: /
    source:
        image: filebrowser/filebrowser:latest
    ports:
        - name: http
          hostPort: 8080
          servicePort: 80
          protocol: tcp
"#;

    let manifest = parse_service_manifest_yaml(yaml, Path::new("/tmp"), Path::new("/tmp")).unwrap();
    let expose = manifest.expose.expect("expected expose config");
    assert_eq!(expose.transport.kind, ServiceExposeTransportKind::Tcp);
    let usage = expose.usage.expect("expected usage config");
    assert_eq!(usage.kind, ServiceExposeUsageKind::Web);
    assert_eq!(usage.path.as_deref(), Some("/"));
}

#[test]
fn parse_manifest_expose_disabled_returns_none() {
    let yaml = r#"
apiVersion: fungi.rs/v1alpha1
kind: ServiceManifest
metadata:
    name: raw-service
spec:
    runtime: docker
    expose:
        enabled: false
        transport:
            kind: tcp
    source:
        image: example/raw:latest
"#;

    let manifest = parse_service_manifest_yaml(yaml, Path::new("/tmp"), Path::new("/tmp")).unwrap();
    assert!(manifest.expose.is_none());
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
