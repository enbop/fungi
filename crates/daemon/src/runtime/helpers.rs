use std::{
    fs,
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use fungi_config::paths::FungiPaths;
use fungi_docker_agent::{ContainerSpec, DockerAgentError, PortProtocol};
use tokio::process::Command;

use super::{
    manifest::service_expose_endpoint_bindings, model::*, providers::WasmtimeServiceState,
};

pub(crate) fn docker_spec_from_manifest(manifest: &ServiceManifest) -> Result<ContainerSpec> {
    if manifest.runtime != RuntimeKind::Docker {
        bail!("service manifest runtime does not match docker provider")
    }

    let ServiceSource::Docker { image } = &manifest.source else {
        bail!("docker runtime requires a docker image source")
    };

    Ok(ContainerSpec {
        name: Some(manifest.name.clone()),
        image: image.clone(),
        env: manifest.env.clone(),
        mounts: manifest
            .mounts
            .iter()
            .map(|mount| fungi_docker_agent::BindMount {
                host_path: mount.host_path.clone(),
                container_path: mount.runtime_path.clone(),
            })
            .collect(),
        ports: manifest
            .ports
            .iter()
            .map(|port| fungi_docker_agent::PortBinding {
                host_port: port.host_port,
                container_port: port.service_port,
                protocol: match port.protocol {
                    ServicePortProtocol::Tcp => PortProtocol::Tcp,
                    ServicePortProtocol::Udp => PortProtocol::Udp,
                },
            })
            .collect(),
        command: manifest.command.clone(),
        entrypoint: manifest.entrypoint.clone(),
        working_dir: manifest.working_dir.clone(),
        labels: manifest.labels.clone(),
    })
}

fn ensure_wasmtime_manifest(manifest: &ServiceManifest) -> Result<()> {
    if manifest.runtime != RuntimeKind::Wasmtime {
        bail!("service manifest runtime does not match wasmtime provider")
    }

    match &manifest.source {
        ServiceSource::WasmtimeFile { component } => {
            if component.as_os_str().is_empty() {
                bail!("wasmtime component path must not be empty");
            }
            Ok(())
        }
        ServiceSource::WasmtimeUrl { url } => {
            if url.trim().is_empty() {
                bail!("wasmtime source url must not be empty");
            }
            Ok(())
        }
        ServiceSource::Docker { .. } => bail!("wasmtime runtime requires a wasm component source"),
        ServiceSource::TcpLink { .. } => bail!("wasmtime runtime requires a wasm component source"),
    }
}

pub(crate) fn ensure_manifest_mount_dirs(manifest: &ServiceManifest) -> Result<()> {
    for mount in &manifest.mounts {
        fs::create_dir_all(&mount.host_path).with_context(|| {
            format!(
                "Failed to create service host mount directory: {}",
                mount.host_path.display()
            )
        })?;
    }
    Ok(())
}

pub(crate) fn ensure_services_root_exists(fungi_home: &Path) -> Result<()> {
    let paths = FungiPaths::from_fungi_home(fungi_home);
    let services_root = paths.services_root();
    fs::create_dir_all(&services_root).with_context(|| {
        format!(
            "Failed to create services root directory: {}",
            services_root.display()
        )
    })?;
    let service_appdata_root = paths.service_appdata_root();
    fs::create_dir_all(&service_appdata_root).with_context(|| {
        format!(
            "Failed to create service appdata root directory: {}",
            service_appdata_root.display()
        )
    })?;
    let service_artifacts_root = paths.service_artifacts_root();
    fs::create_dir_all(&service_artifacts_root).with_context(|| {
        format!(
            "Failed to create service artifacts root directory: {}",
            service_artifacts_root.display()
        )
    })?;
    let user_home = paths.user_home();
    fs::create_dir_all(&user_home).with_context(|| {
        format!(
            "Failed to create Fungi user workspace: {}",
            user_home.display()
        )
    })?;
    Ok(())
}

pub(crate) fn is_missing_docker_container_error(error: &anyhow::Error) -> bool {
    matches!(
        error.downcast_ref::<DockerAgentError>(),
        Some(DockerAgentError::DockerApi { status, .. }) if status.as_u16() == 404
    )
}

fn validate_allowed_host_paths(
    manifest: &ServiceManifest,
    allowed_host_paths: &[PathBuf],
) -> Result<()> {
    let normalized_roots = allowed_host_paths
        .iter()
        .map(|path| normalize_absolute_path(path))
        .collect::<Result<Vec<_>>>()?;

    for mount in &manifest.mounts {
        let host_path = normalize_absolute_path(&mount.host_path)?;
        let allowed = normalized_roots
            .iter()
            .any(|allowed_root| host_path.starts_with(allowed_root));
        if !allowed {
            bail!(
                "wasmtime host path is outside allowed roots: {}",
                mount.host_path.display()
            );
        }
    }

    Ok(())
}

async fn stage_wasmtime_component(
    manifest: &ServiceManifest,
    service_dir: &Path,
) -> Result<PathBuf> {
    let target_path = service_dir.join("component.wasm");
    match &manifest.source {
        ServiceSource::WasmtimeFile { component } => {
            fs::copy(component, &target_path).with_context(|| {
                format!(
                    "Failed to copy WASI component from {} to {}",
                    component.display(),
                    target_path.display()
                )
            })?;
        }
        ServiceSource::WasmtimeUrl { url } => {
            let response = reqwest::get(url)
                .await
                .with_context(|| format!("Failed to download WASI component from {url}"))?
                .error_for_status()
                .with_context(|| format!("WASI component download returned error for {url}"))?;
            let bytes = response
                .bytes()
                .await
                .with_context(|| format!("Failed to read WASI download body from {url}"))?;
            fs::write(&target_path, &bytes).with_context(|| {
                format!(
                    "Failed to write staged WASI component: {}",
                    target_path.display()
                )
            })?;
        }
        ServiceSource::Docker { .. } | ServiceSource::TcpLink { .. } => {
            bail!("invalid wasmtime source type")
        }
    }
    Ok(target_path)
}

pub(crate) async fn build_wasmtime_state(
    runtime_root: &Path,
    service_artifacts_dir: &Path,
    allowed_host_paths: &[PathBuf],
    manifest: &ServiceManifest,
    restage_component: bool,
) -> Result<WasmtimeServiceState> {
    ensure_wasmtime_manifest(manifest)?;
    validate_allowed_host_paths(manifest, allowed_host_paths)?;

    fs::create_dir_all(service_artifacts_dir).with_context(|| {
        format!(
            "Failed to create service artifacts directory: {}",
            service_artifacts_dir.display()
        )
    })?;
    ensure_manifest_mount_dirs(manifest)?;

    let staged_component_path = service_artifacts_dir.join("component.wasm");
    let staged_component_path = if restage_component || !staged_component_path.exists() {
        stage_wasmtime_component(manifest, service_artifacts_dir).await?
    } else {
        staged_component_path
    };

    let runtime_dir = runtime_root.join("wasmtime").join(&manifest.name);
    fs::create_dir_all(&runtime_dir).with_context(|| {
        format!(
            "Failed to create runtime directory: {}",
            runtime_dir.display()
        )
    })?;
    let log_file_path = runtime_dir.join("runtime.log");
    if !log_file_path.exists() {
        fs::File::create(&log_file_path)
            .with_context(|| format!("Failed to create log file: {}", log_file_path.display()))?;
    }

    Ok(WasmtimeServiceState {
        manifest: manifest.clone(),
        source_display: source_display(&manifest.source),
        staged_component_path,
        service_dir: service_artifacts_dir.to_path_buf(),
        runtime_dir,
        log_file_path,
        child: None,
        last_exit_code: None,
    })
}

pub(crate) fn build_wasmtime_command(
    launcher_path: &Path,
    fungi_home: &Path,
    state: &WasmtimeServiceState,
) -> Result<Command> {
    let mut command = Command::new(launcher_path);
    command.kill_on_drop(true);
    command.arg("--fungi-dir");
    command.arg(fungi_home.as_os_str());

    if should_serve_wasmtime_http(&state.manifest) {
        let port = state.manifest.ports[0].host_port;
        command.arg("serve");
        command.arg(format!("--addr=127.0.0.1:{port}"));
    } else {
        command.arg("run");
    }
    command.arg("-Scli");
    if has_tcp_ports(&state.manifest) {
        command.arg("-Stcp");
        command.arg("-Sinherit-network");
        command.arg("-Sallow-ip-name-lookup");
    }

    for mount in &state.manifest.mounts {
        command.arg("--dir");
        let guest = mount.runtime_path.trim().trim_start_matches('/');
        if guest.is_empty() {
            command.arg(mount.host_path.as_os_str());
        } else {
            command.arg(format!("{}::{}", mount.host_path.display(), guest));
        }
    }

    command.arg(&state.staged_component_path);
    for arg in &state.manifest.command {
        command.arg(arg);
    }
    if let Some(working_dir) = &state.manifest.working_dir {
        command.current_dir(working_dir);
    } else {
        command.current_dir(&state.service_dir);
    }
    command.envs(&state.manifest.env);
    Ok(command)
}

fn should_serve_wasmtime_http(manifest: &ServiceManifest) -> bool {
    manifest.run_mode == ServiceRunMode::Http
}

fn has_tcp_ports(manifest: &ServiceManifest) -> bool {
    manifest
        .ports
        .iter()
        .any(|port| port.protocol == ServicePortProtocol::Tcp)
}

pub(crate) fn refresh_child_state(state: &mut WasmtimeServiceState) -> Result<()> {
    if let Some(child) = state.child.as_mut()
        && let Some(status) = child
            .try_wait()
            .context("Failed to query fungi WASI process status")?
    {
        state.last_exit_code = status.code();
        state.child = None;
    }
    Ok(())
}

pub(crate) fn map_docker_instance(
    details: fungi_docker_agent::ContainerDetails,
) -> ServiceInstance {
    ServiceInstance {
        id: details.id.clone(),
        runtime: RuntimeKind::Docker,
        name: details.name,
        source: details.image,
        labels: details.labels,
        ports: Vec::new(),
        exposed_endpoints: Vec::new(),
        status: ServiceStatus {
            state: details.state.status,
            running: details.state.running,
        },
    }
}

pub(crate) fn map_wasmtime_instance(handle: &str, state: &WasmtimeServiceState) -> ServiceInstance {
    let running = state.child.is_some();
    let status = if running {
        if state.manifest.ports.is_empty() {
            "running".to_string()
        } else {
            "serving".to_string()
        }
    } else if let Some(code) = state.last_exit_code {
        format!("exited({code})")
    } else {
        "created".to_string()
    };

    ServiceInstance {
        id: format!("wasmtime:{handle}"),
        runtime: RuntimeKind::Wasmtime,
        name: state.manifest.name.clone(),
        source: state.source_display.clone(),
        labels: state.manifest.labels.clone(),
        ports: Vec::new(),
        exposed_endpoints: Vec::new(),
        status: ServiceStatus {
            state: status,
            running,
        },
    }
}

pub(crate) fn missing_instance_from_manifest(manifest: &ServiceManifest) -> ServiceInstance {
    ServiceInstance {
        id: service_instance_id(manifest.runtime, &manifest.name),
        runtime: manifest.runtime,
        name: manifest.name.clone(),
        source: source_display(&manifest.source),
        labels: manifest.labels.clone(),
        ports: manifest.ports.clone(),
        exposed_endpoints: service_expose_endpoint_bindings(manifest),
        status: ServiceStatus {
            state: "missing".to_string(),
            running: false,
        },
    }
}

pub(crate) fn enrich_instance_from_manifest(
    mut instance: ServiceInstance,
    manifest: &ServiceManifest,
) -> ServiceInstance {
    if instance.id.is_empty() {
        instance.id = service_instance_id(manifest.runtime, &manifest.name);
    }
    instance.name = manifest.name.clone();
    instance.ports = manifest.ports.clone();
    instance.exposed_endpoints = service_expose_endpoint_bindings(manifest);
    instance
}

fn service_instance_id(runtime: RuntimeKind, name: &str) -> String {
    let runtime_name = match runtime {
        RuntimeKind::Docker => "docker",
        RuntimeKind::Wasmtime => "wasmtime",
        RuntimeKind::Link => "link",
    };
    format!("{runtime_name}:{name}")
}

fn source_display(source: &ServiceSource) -> String {
    match source {
        ServiceSource::Docker { image } => image.clone(),
        ServiceSource::WasmtimeFile { component } => component.display().to_string(),
        ServiceSource::WasmtimeUrl { url } => url.clone(),
        ServiceSource::TcpLink { host, port } => format!("{host}:{port}"),
    }
}

fn normalize_absolute_path(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute() {
        bail!("host path must be absolute: {}", path.display());
    }

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::Normal(part) => normalized.push(part),
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Prefix(_) => normalized.push(component.as_os_str()),
        }
    }
    Ok(normalized)
}

pub(crate) fn tail_lines(text: &str, tail: Option<&str>) -> String {
    let Some(tail) = tail else {
        return text.to_string();
    };
    let Ok(count) = tail.parse::<usize>() else {
        return text.to_string();
    };
    let lines = text.lines().collect::<Vec<_>>();
    let start = lines.len().saturating_sub(count);
    let mut output = lines[start..].join("\n");
    if text.ends_with('\n') && !output.is_empty() {
        output.push('\n');
    }
    output
}
