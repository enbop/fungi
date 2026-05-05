use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    net::{TcpListener as StdTcpListener, UdpSocket as StdUdpSocket},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use fungi_config::paths::FungiPaths;
use fungi_util::protocols::service_port_protocol;

use super::model::*;

pub fn load_service_manifest_yaml_file(path: &Path, fungi_home: &Path) -> Result<ServiceManifest> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read service manifest: {}", path.display()))?;
    let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
    parse_service_manifest_yaml(&content, base_dir, fungi_home)
}

pub fn parse_service_manifest_yaml(
    content: &str,
    base_dir: &Path,
    fungi_home: &Path,
) -> Result<ServiceManifest> {
    parse_service_manifest_yaml_with_policy(
        content,
        base_dir,
        fungi_home,
        &ManifestResolutionPolicy::default(),
        &BTreeSet::new(),
    )
}

pub(crate) fn parse_managed_service_manifest_yaml(
    content: &str,
    base_dir: &Path,
    fungi_home: &Path,
    local_service_id: &str,
) -> Result<ServiceManifest> {
    parse_service_manifest_yaml_with_policy_for_service_paths(
        content,
        base_dir,
        fungi_home,
        &ManifestPathRoots::for_local_service_id(fungi_home, local_service_id),
        &ManifestResolutionPolicy::default(),
        &BTreeSet::new(),
    )
}

pub fn parse_service_manifest_yaml_with_policy(
    content: &str,
    base_dir: &Path,
    fungi_home: &Path,
    policy: &ManifestResolutionPolicy,
    used_host_ports: &BTreeSet<u16>,
) -> Result<ServiceManifest> {
    let document: ServiceManifestDocument =
        serde_yaml::from_str(content).context("Failed to parse service manifest YAML")?;
    document.into_service_manifest_for_node(base_dir, fungi_home, policy, used_host_ports)
}

pub(crate) fn parse_service_manifest_yaml_with_policy_for_service_paths(
    content: &str,
    base_dir: &Path,
    fungi_home: &Path,
    path_roots: &ManifestPathRoots,
    policy: &ManifestResolutionPolicy,
    used_host_ports: &BTreeSet<u16>,
) -> Result<ServiceManifest> {
    let document: ServiceManifestDocument =
        serde_yaml::from_str(content).context("Failed to parse service manifest YAML")?;
    document.into_service_manifest_for_node_with_service_paths(
        base_dir,
        fungi_home,
        path_roots,
        policy,
        used_host_ports,
    )
}

pub(crate) fn peek_service_manifest_name(content: &str) -> Result<String> {
    let document: ServiceManifestDocument =
        serde_yaml::from_str(content).context("Failed to parse service manifest YAML")?;
    normalize_non_empty(&document.metadata.name, "metadata.name")
}

pub fn service_manifest_to_yaml(manifest: &ServiceManifest) -> Result<String> {
    let run = match &manifest.source {
        ServiceSource::Docker { image } => Some(ServiceManifestRun {
            docker: Some(ServiceManifestDockerRun {
                image: image.clone(),
            }),
            wasmtime: None,
        }),
        ServiceSource::WasmtimeFile { component } => Some(ServiceManifestRun {
            docker: None,
            wasmtime: Some(ServiceManifestWasmtimeRun {
                file: Some(component.display().to_string()),
                url: None,
            }),
        }),
        ServiceSource::WasmtimeUrl { url } => Some(ServiceManifestRun {
            docker: None,
            wasmtime: Some(ServiceManifestWasmtimeRun {
                file: None,
                url: Some(url.clone()),
            }),
        }),
        ServiceSource::TcpLink { .. } => None,
    };
    let entries = manifest_entries_to_document(manifest);

    let document = ServiceManifestDocument {
        api_version: "fungi.rs/v1alpha1".to_string(),
        kind: "Service".to_string(),
        metadata: ServiceManifestMetadata {
            name: manifest.name.clone(),
            labels: manifest.labels.clone(),
        },
        spec: ServiceManifestSpec {
            run,
            entries,
            env: manifest.env.clone(),
            mounts: manifest
                .mounts
                .iter()
                .map(|mount| ServiceManifestMount {
                    host_path: mount.host_path.display().to_string(),
                    runtime_path: mount.runtime_path.clone(),
                })
                .collect(),
            command: manifest.command.clone(),
            entrypoint: manifest.entrypoint.clone(),
            working_dir: manifest.working_dir.clone(),
        },
    };

    serde_yaml::to_string(&document).context("Failed to encode service manifest YAML")
}

impl ServiceManifestDocument {
    pub fn into_service_manifest(
        self,
        base_dir: &Path,
        fungi_home: &Path,
    ) -> Result<ServiceManifest> {
        self.into_service_manifest_for_node(
            base_dir,
            fungi_home,
            &ManifestResolutionPolicy::default(),
            &BTreeSet::new(),
        )
    }

    pub fn into_service_manifest_for_node(
        self,
        base_dir: &Path,
        fungi_home: &Path,
        policy: &ManifestResolutionPolicy,
        used_host_ports: &BTreeSet<u16>,
    ) -> Result<ServiceManifest> {
        let service_name = normalize_non_empty(&self.metadata.name, "metadata.name")?;
        let path_roots = ManifestPathRoots::for_local_service_id(fungi_home, &service_name);
        self.into_service_manifest_for_node_with_service_paths(
            base_dir,
            fungi_home,
            &path_roots,
            policy,
            used_host_ports,
        )
    }

    pub(crate) fn into_service_manifest_for_node_with_service_paths(
        self,
        base_dir: &Path,
        fungi_home: &Path,
        path_roots: &ManifestPathRoots,
        _policy: &ManifestResolutionPolicy,
        used_host_ports: &BTreeSet<u16>,
    ) -> Result<ServiceManifest> {
        if self.kind != "Service" {
            bail!("Unsupported manifest kind: {}", self.kind);
        }

        let ServiceManifestDocument {
            api_version: _,
            kind: _,
            metadata,
            spec,
        } = self;
        let service_name = normalize_non_empty(&metadata.name, "metadata.name")?;
        let metadata_labels = metadata.labels;
        let mut reserved_host_ports = used_host_ports.clone();
        if spec.entries.is_empty() {
            bail!("service manifest requires at least one spec.entries item");
        }

        let runtime_and_source =
            parse_manifest_run(spec.run, &spec.entries, base_dir, fungi_home, path_roots)?;
        let ports = parse_manifest_entries(
            &spec.entries,
            runtime_and_source.runtime,
            &mut reserved_host_ports,
        )?;
        let expose = parse_manifest_entries_expose(&spec.entries)?;

        Ok(ServiceManifest {
            name: service_name.clone(),
            runtime: runtime_and_source.runtime,
            source: runtime_and_source.source,
            expose,
            env: spec.env,
            mounts: spec
                .mounts
                .into_iter()
                .map(|mount| ServiceMount {
                    host_path: resolve_manifest_path(
                        &mount.host_path,
                        base_dir,
                        fungi_home,
                        path_roots,
                    ),
                    runtime_path: mount.runtime_path,
                })
                .collect(),
            ports,
            command: spec.command,
            entrypoint: spec.entrypoint,
            working_dir: spec.working_dir.map(|value| {
                resolve_manifest_path_string(value.as_str(), base_dir, fungi_home, path_roots)
            }),
            labels: metadata_labels,
        })
    }
}

struct RuntimeAndSource {
    runtime: RuntimeKind,
    source: ServiceSource,
}

fn manifest_entries_to_document(
    manifest: &ServiceManifest,
) -> BTreeMap<String, ServiceManifestEntry> {
    let usage = manifest
        .expose
        .as_ref()
        .and_then(|expose| expose.usage.as_ref())
        .map(|usage| manifest_usage_to_entry_usage(usage.kind));
    let path = manifest
        .expose
        .as_ref()
        .and_then(|expose| expose.usage.as_ref())
        .and_then(|usage| usage.path.clone());
    let icon_url = manifest
        .expose
        .as_ref()
        .and_then(|expose| expose.icon_url.clone());
    let catalog_id = manifest
        .expose
        .as_ref()
        .and_then(|expose| expose.catalog_id.clone());

    let mut entries = BTreeMap::new();
    for (index, port) in manifest.ports.iter().enumerate() {
        let fallback_name = if index == 0 {
            "main".to_string()
        } else {
            format!("main-{index}")
        };
        let name = port.name.clone().unwrap_or(fallback_name);
        let protocol = (port.protocol != ServicePortProtocol::Tcp).then_some(port.protocol);
        let entry = match &manifest.source {
            ServiceSource::TcpLink { host, port } => ServiceManifestEntry {
                target: Some(format!("{host}:{port}")),
                port: None,
                protocol,
                usage,
                path: path.clone(),
                icon_url: icon_url.clone(),
                catalog_id: catalog_id.clone(),
            },
            _ => ServiceManifestEntry {
                target: None,
                port: Some(port.service_port),
                protocol,
                usage,
                path: path.clone(),
                icon_url: icon_url.clone(),
                catalog_id: catalog_id.clone(),
            },
        };
        entries.insert(name, entry);
    }
    entries
}

fn manifest_usage_to_entry_usage(kind: ServiceExposeUsageKind) -> ServiceManifestEntryUsageKind {
    match kind {
        ServiceExposeUsageKind::Web => ServiceManifestEntryUsageKind::Web,
        ServiceExposeUsageKind::Ssh => ServiceManifestEntryUsageKind::Ssh,
        ServiceExposeUsageKind::Raw => ServiceManifestEntryUsageKind::Tcp,
    }
}

fn entry_usage_to_manifest_usage(kind: ServiceManifestEntryUsageKind) -> ServiceExposeUsageKind {
    match kind {
        ServiceManifestEntryUsageKind::Web => ServiceExposeUsageKind::Web,
        ServiceManifestEntryUsageKind::Ssh => ServiceExposeUsageKind::Ssh,
        ServiceManifestEntryUsageKind::Tcp => ServiceExposeUsageKind::Raw,
    }
}

fn parse_manifest_run(
    run: Option<ServiceManifestRun>,
    entries: &BTreeMap<String, ServiceManifestEntry>,
    base_dir: &Path,
    fungi_home: &Path,
    path_roots: &ManifestPathRoots,
) -> Result<RuntimeAndSource> {
    match run {
        Some(run) => parse_runtime_run(run, entries, base_dir, fungi_home, path_roots),
        None => parse_tcp_tunnel_run(entries),
    }
}

fn parse_runtime_run(
    run: ServiceManifestRun,
    entries: &BTreeMap<String, ServiceManifestEntry>,
    base_dir: &Path,
    fungi_home: &Path,
    path_roots: &ManifestPathRoots,
) -> Result<RuntimeAndSource> {
    for (name, entry) in entries {
        if entry.target.is_some() {
            bail!("spec.entries.{name}.target cannot be used when spec.run is set");
        }
        if entry.port.is_none() {
            bail!("spec.entries.{name}.port is required when spec.run is set");
        }
    }

    match (run.docker, run.wasmtime) {
        (Some(docker), None) => {
            let image = normalize_non_empty(&docker.image, "spec.run.docker.image")?;
            Ok(RuntimeAndSource {
                runtime: RuntimeKind::Docker,
                source: ServiceSource::Docker { image },
            })
        }
        (None, Some(wasmtime)) => match (wasmtime.file, wasmtime.url) {
            (Some(file), None) => Ok(RuntimeAndSource {
                runtime: RuntimeKind::Wasmtime,
                source: ServiceSource::WasmtimeFile {
                    component: resolve_manifest_path(&file, base_dir, fungi_home, path_roots),
                },
            }),
            (None, Some(url)) => {
                let url = normalize_non_empty(&url, "spec.run.wasmtime.url")?;
                Ok(RuntimeAndSource {
                    runtime: RuntimeKind::Wasmtime,
                    source: ServiceSource::WasmtimeUrl { url },
                })
            }
            (Some(_), Some(_)) => {
                bail!(
                    "wasmtime service manifest accepts only one of spec.run.wasmtime.file or spec.run.wasmtime.url"
                )
            }
            (None, None) => {
                bail!(
                    "wasmtime service manifest requires spec.run.wasmtime.file or spec.run.wasmtime.url"
                )
            }
        },
        (Some(_), Some(_)) => {
            bail!("service manifest accepts only one runtime under spec.run")
        }
        (None, None) => bail!("spec.run requires docker or wasmtime"),
    }
}

fn parse_tcp_tunnel_run(
    entries: &BTreeMap<String, ServiceManifestEntry>,
) -> Result<RuntimeAndSource> {
    if entries.len() != 1 {
        bail!("tcp tunnel service manifests currently support exactly one entry");
    }
    let (name, entry) = entries.iter().next().expect("entries is non-empty");
    if entry.target.is_some() && entry.port.is_some() {
        bail!("spec.entries.{name} must use either target or port, not both");
    }
    if entry.port.is_some() {
        bail!("spec.entries.{name}.port cannot be used without spec.run");
    }
    let Some(target) = entry.target.as_deref() else {
        bail!("spec.entries.{name} requires target or port");
    };
    let protocol = entry.protocol.unwrap_or(ServicePortProtocol::Tcp);
    if protocol != ServicePortProtocol::Tcp {
        bail!("spec.entries.{name}.target currently supports only protocol: tcp");
    }
    let (host, port) = parse_tcp_target(target, &format!("spec.entries.{name}.target"))?;
    if !matches!(host.as_str(), "127.0.0.1" | "localhost") {
        bail!("spec.entries.{name}.target currently supports only 127.0.0.1 or localhost");
    }
    Ok(RuntimeAndSource {
        runtime: RuntimeKind::Link,
        source: ServiceSource::TcpLink { host, port },
    })
}

fn parse_manifest_entries(
    entries: &BTreeMap<String, ServiceManifestEntry>,
    runtime: RuntimeKind,
    reserved_host_ports: &mut BTreeSet<u16>,
) -> Result<Vec<ServicePort>> {
    entries
        .iter()
        .map(|(name, entry)| parse_manifest_entry(name, entry, runtime, reserved_host_ports))
        .collect()
}

fn parse_manifest_entry(
    name: &str,
    entry: &ServiceManifestEntry,
    runtime: RuntimeKind,
    reserved_host_ports: &mut BTreeSet<u16>,
) -> Result<ServicePort> {
    let name = normalize_non_empty(name, "spec.entries key")?;
    let protocol = entry.protocol.unwrap_or(ServicePortProtocol::Tcp);
    if protocol != ServicePortProtocol::Tcp {
        bail!("spec.entries.{name}.protocol currently supports only tcp");
    }

    match (entry.target.as_deref(), entry.port) {
        (Some(_), Some(_)) => {
            bail!("spec.entries.{name} must use either target or port, not both");
        }
        (Some(target), None) => {
            if runtime != RuntimeKind::Link {
                bail!("spec.entries.{name}.target cannot be used when spec.run is set");
            }
            let (_host, port) = parse_tcp_target(target, &format!("spec.entries.{name}.target"))?;
            Ok(ServicePort {
                name: Some(name),
                host_port: port,
                host_port_allocation: ServicePortAllocation::Fixed,
                service_port: port,
                protocol,
            })
        }
        (None, Some(service_port)) => {
            if runtime == RuntimeKind::Link {
                bail!("spec.entries.{name}.port cannot be used without spec.run");
            }
            if service_port == 0 {
                bail!("spec.entries.{name}.port must be greater than 0");
            }
            let resolved_port = allocate_auto_host_port(protocol, reserved_host_ports)?;
            Ok(ServicePort {
                name: Some(name),
                host_port: resolved_port.port,
                host_port_allocation: resolved_port.allocation,
                service_port,
                protocol,
            })
        }
        (None, None) => bail!("spec.entries.{name} requires target or port"),
    }
}

fn parse_manifest_entries_expose(
    entries: &BTreeMap<String, ServiceManifestEntry>,
) -> Result<Option<ServiceExpose>> {
    let Some((first_name, first_entry)) = entries.iter().next() else {
        return Ok(None);
    };
    let first_metadata = entry_expose_metadata(first_entry);
    for (name, entry) in entries.iter().skip(1) {
        let metadata = entry_expose_metadata(entry);
        if metadata != first_metadata {
            bail!(
                "spec.entries.{name} expose metadata must match spec.entries.{first_name}; per-entry usage/path/iconUrl/catalogId is not supported yet"
            );
        }
    }

    let usage = first_metadata.usage.map(|kind| ServiceExposeUsage {
        kind: entry_usage_to_manifest_usage(kind),
        path: first_metadata.path.clone(),
    });
    Ok(Some(ServiceExpose {
        transport: ServiceExposeTransport {
            kind: ServiceExposeTransportKind::Tcp,
        },
        usage,
        icon_url: first_metadata.icon_url.clone(),
        catalog_id: first_metadata.catalog_id.clone(),
    }))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EntryExposeMetadata {
    usage: Option<ServiceManifestEntryUsageKind>,
    path: Option<String>,
    icon_url: Option<String>,
    catalog_id: Option<String>,
}

fn entry_expose_metadata(entry: &ServiceManifestEntry) -> EntryExposeMetadata {
    EntryExposeMetadata {
        usage: entry.usage,
        path: normalize_optional(entry.path.clone()),
        icon_url: normalize_optional(entry.icon_url.clone()),
        catalog_id: normalize_optional(entry.catalog_id.clone()),
    }
}

fn parse_tcp_target(value: &str, field_name: &str) -> Result<(String, u16)> {
    let value = normalize_non_empty(value, field_name)?;
    let Some((host, port)) = value.rsplit_once(':') else {
        bail!("{field_name} must use host:port");
    };
    let host = normalize_non_empty(host, field_name)?;
    let port = port
        .parse::<u16>()
        .with_context(|| format!("{field_name} port must be a number"))?;
    if port == 0 {
        bail!("{field_name} port must be greater than 0");
    }
    Ok((host, port))
}

fn normalize_non_empty(value: &str, field_name: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("{field_name} must not be empty");
    }
    Ok(trimmed.to_string())
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

pub fn service_expose_endpoint_bindings(
    manifest: &ServiceManifest,
) -> Vec<ServiceExposeEndpointBinding> {
    let Some(expose) = &manifest.expose else {
        return Vec::new();
    };

    if expose.transport.kind != ServiceExposeTransportKind::Tcp {
        return Vec::new();
    }

    let mut endpoints = manifest
        .ports
        .iter()
        .filter(|port| port.protocol == ServicePortProtocol::Tcp)
        .filter_map(|port| {
            let name = port.name.as_ref()?.trim();
            if name.is_empty() {
                return None;
            }

            Some(ServiceExposeEndpointBinding {
                name: name.to_string(),
                protocol: service_port_protocol(&manifest.name, name),
                host_port: port.host_port,
                service_port: port.service_port,
            })
        })
        .collect::<Vec<_>>();

    endpoints.sort_by(|left, right| left.name.cmp(&right.name));
    endpoints
}

struct ResolvedManifestHostPort {
    port: u16,
    allocation: ServicePortAllocation,
}

fn allocate_auto_host_port(
    protocol: ServicePortProtocol,
    reserved_host_ports: &mut BTreeSet<u16>,
) -> Result<ResolvedManifestHostPort> {
    for _ in 0..64 {
        let port = reserve_ephemeral_host_port(protocol)?;
        if reserved_host_ports.insert(port) {
            return Ok(ResolvedManifestHostPort {
                port,
                allocation: ServicePortAllocation::Auto,
            });
        }
    }

    bail!("failed to allocate host port automatically from the operating system")
}

fn reserve_ephemeral_host_port(protocol: ServicePortProtocol) -> Result<u16> {
    match protocol {
        ServicePortProtocol::Tcp => {
            let listener = StdTcpListener::bind(("127.0.0.1", 0))
                .context("failed to reserve an automatic TCP host port")?;
            Ok(listener.local_addr()?.port())
        }
        ServicePortProtocol::Udp => {
            let socket = StdUdpSocket::bind(("127.0.0.1", 0))
                .context("failed to reserve an automatic UDP host port")?;
            Ok(socket.local_addr()?.port())
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ManifestPathRoots {
    service_appdata_dir: PathBuf,
    service_artifacts_dir: PathBuf,
    user_root_dir: PathBuf,
    user_home_dir: PathBuf,
}

impl ManifestPathRoots {
    pub(crate) fn for_local_service_id(fungi_home: &Path, local_service_id: &str) -> Self {
        let paths = FungiPaths::from_fungi_home(fungi_home);
        Self {
            service_appdata_dir: paths.service_appdata_dir(local_service_id),
            service_artifacts_dir: paths.service_artifacts_dir(local_service_id),
            user_root_dir: paths.user_root(),
            user_home_dir: paths.user_home(),
        }
    }
}

fn resolve_manifest_path(
    path: &str,
    base_dir: &Path,
    fungi_home: &Path,
    path_roots: &ManifestPathRoots,
) -> PathBuf {
    let expanded = resolve_manifest_path_string(path, base_dir, fungi_home, path_roots);
    PathBuf::from(expanded)
}

fn resolve_manifest_path_string(
    path: &str,
    base_dir: &Path,
    fungi_home: &Path,
    path_roots: &ManifestPathRoots,
) -> String {
    let fungi_home_value = fungi_home.to_string_lossy();
    let service_appdata_value = path_roots.service_appdata_dir.to_string_lossy();
    let service_artifacts_value = path_roots.service_artifacts_dir.to_string_lossy();
    let user_root_value = path_roots.user_root_dir.to_string_lossy();
    let user_home_value = path_roots.user_home_dir.to_string_lossy();
    let expanded = path
        .replace("${FUNGI_HOME}", &fungi_home_value)
        .replace("$FUNGI_HOME", &fungi_home_value)
        .replace("${fungi_home}", &fungi_home_value)
        .replace("$fungi_home", &fungi_home_value)
        .replace("${SERVICE_APPDATA}", &service_appdata_value)
        .replace("$SERVICE_APPDATA", &service_appdata_value)
        .replace("${service_appdata}", &service_appdata_value)
        .replace("$service_appdata", &service_appdata_value)
        .replace("${SERVICE_ARTIFACTS}", &service_artifacts_value)
        .replace("$SERVICE_ARTIFACTS", &service_artifacts_value)
        .replace("${service_artifacts}", &service_artifacts_value)
        .replace("$service_artifacts", &service_artifacts_value)
        .replace("${USER_ROOT}", &user_root_value)
        .replace("$USER_ROOT", &user_root_value)
        .replace("${user_root}", &user_root_value)
        .replace("$user_root", &user_root_value)
        .replace("${USER_HOME}", &user_home_value)
        .replace("$USER_HOME", &user_home_value)
        .replace("${user_home}", &user_home_value)
        .replace("$user_home", &user_home_value);
    let resolved = PathBuf::from(&expanded);
    if resolved.is_absolute() {
        resolved.to_string_lossy().to_string()
    } else {
        base_dir.join(resolved).to_string_lossy().to_string()
    }
}
