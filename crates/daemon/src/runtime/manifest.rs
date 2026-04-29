use std::{
    collections::BTreeSet,
    fs,
    net::{TcpListener as StdTcpListener, UdpSocket as StdUdpSocket},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
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
    service_data_dir: &Path,
) -> Result<ServiceManifest> {
    parse_service_manifest_yaml_with_policy_for_service_data_dir(
        content,
        base_dir,
        fungi_home,
        service_data_dir,
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

pub(crate) fn parse_service_manifest_yaml_with_policy_for_service_data_dir(
    content: &str,
    base_dir: &Path,
    fungi_home: &Path,
    service_data_dir: &Path,
    policy: &ManifestResolutionPolicy,
    used_host_ports: &BTreeSet<u16>,
) -> Result<ServiceManifest> {
    let document: ServiceManifestDocument =
        serde_yaml::from_str(content).context("Failed to parse service manifest YAML")?;
    document.into_service_manifest_for_node_with_service_data_dir(
        base_dir,
        fungi_home,
        service_data_dir,
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
    let source = match &manifest.source {
        ServiceSource::Docker { image } => ServiceManifestSource {
            image: Some(image.clone()),
            ..ServiceManifestSource::default()
        },
        ServiceSource::WasmtimeFile { component } => ServiceManifestSource {
            file: Some(component.display().to_string()),
            ..ServiceManifestSource::default()
        },
        ServiceSource::WasmtimeUrl { url } => ServiceManifestSource {
            url: Some(url.clone()),
            ..ServiceManifestSource::default()
        },
        ServiceSource::TcpLink { host, port } => ServiceManifestSource {
            host: Some(host.clone()),
            port: Some(*port),
            ..ServiceManifestSource::default()
        },
    };

    let document = ServiceManifestDocument {
        api_version: "fungi.rs/v1alpha1".to_string(),
        kind: "ServiceManifest".to_string(),
        metadata: ServiceManifestMetadata {
            name: manifest.name.clone(),
            labels: manifest.labels.clone(),
        },
        spec: ServiceManifestSpec {
            runtime: manifest.runtime,
            source,
            expose: manifest.expose.clone().map(|expose| ServiceManifestExpose {
                enabled: true,
                transport: Some(ServiceManifestExposeTransport {
                    kind: expose.transport.kind,
                }),
                usage: expose.usage.map(|usage| ServiceManifestExposeUsage {
                    kind: usage.kind,
                    path: usage.path,
                }),
                icon_url: expose.icon_url,
                catalog_id: expose.catalog_id,
            }),
            env: manifest.env.clone(),
            mounts: manifest
                .mounts
                .iter()
                .map(|mount| ServiceManifestMount {
                    host_path: mount.host_path.display().to_string(),
                    runtime_path: mount.runtime_path.clone(),
                })
                .collect(),
            ports: manifest
                .ports
                .iter()
                .map(|port| ServiceManifestPort {
                    host_port: Some(ServiceManifestHostPort::Fixed(port.host_port)),
                    service_port: port.service_port,
                    name: port.name.clone(),
                    protocol: port.protocol,
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
        let service_data_dir = fungi_home.join("data").join(service_name);
        self.into_service_manifest_for_node_with_service_data_dir(
            base_dir,
            fungi_home,
            &service_data_dir,
            policy,
            used_host_ports,
        )
    }

    pub(crate) fn into_service_manifest_for_node_with_service_data_dir(
        self,
        base_dir: &Path,
        fungi_home: &Path,
        service_data_dir: &Path,
        _policy: &ManifestResolutionPolicy,
        used_host_ports: &BTreeSet<u16>,
    ) -> Result<ServiceManifest> {
        if self.kind != "ServiceManifest" {
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

        let runtime = spec.runtime;
        let source = match runtime {
            RuntimeKind::Docker => {
                let Some(image) = spec.source.image else {
                    bail!("docker service manifest requires spec.source.image");
                };
                ServiceSource::Docker { image }
            }
            RuntimeKind::Wasmtime => match (spec.source.file, spec.source.url) {
                (Some(file), None) => ServiceSource::WasmtimeFile {
                    component: resolve_manifest_path(
                        &file,
                        base_dir,
                        fungi_home,
                        &service_data_dir,
                    ),
                },
                (None, Some(url)) => ServiceSource::WasmtimeUrl { url },
                (Some(_), Some(_)) => {
                    bail!(
                        "wasmtime service manifest accepts only one of spec.source.file or spec.source.url"
                    )
                }
                (None, None) => {
                    bail!("wasmtime service manifest requires spec.source.file or spec.source.url")
                }
            },
            RuntimeKind::Link => {
                let host = normalize_non_empty(
                    spec.source.host.as_deref().unwrap_or("127.0.0.1"),
                    "spec.source.host",
                )?;
                let Some(port) = spec.source.port else {
                    bail!("link service manifest requires spec.source.port");
                };
                if port == 0 {
                    bail!("link service source port must be greater than 0");
                }
                ServiceSource::TcpLink { host, port }
            }
        };

        let ports = spec
            .ports
            .into_iter()
            .map(|port| {
                let resolved_port = resolve_manifest_host_port(
                    port.host_port,
                    port.protocol,
                    &mut reserved_host_ports,
                )?;
                Ok(ServicePort {
                    name: normalize_optional(port.name),
                    host_port: resolved_port.port,
                    host_port_allocation: resolved_port.allocation,
                    service_port: port.service_port,
                    protocol: port.protocol,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        let expose = parse_manifest_expose(spec.expose, &service_name)?;
        validate_manifest_exposed_ports(expose.as_ref(), &ports)?;

        Ok(ServiceManifest {
            name: service_name.clone(),
            runtime,
            source,
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
                        &service_data_dir,
                    ),
                    runtime_path: mount.runtime_path,
                })
                .collect(),
            ports,
            command: spec.command,
            entrypoint: spec.entrypoint,
            working_dir: spec.working_dir.map(|value| {
                resolve_manifest_path_string(
                    value.as_str(),
                    base_dir,
                    fungi_home,
                    &service_data_dir,
                )
            }),
            labels: metadata_labels,
        })
    }
}

fn parse_manifest_expose(
    expose: Option<ServiceManifestExpose>,
    _manifest_name: &str,
) -> Result<Option<ServiceExpose>> {
    let Some(expose) = expose else {
        return Ok(None);
    };

    if !expose.enabled {
        return Ok(None);
    }

    let transport = expose.transport.ok_or_else(|| {
        anyhow::anyhow!("spec.expose.transport is required when expose.enabled=true")
    })?;
    let icon_url = normalize_optional(expose.icon_url);
    let catalog_id = normalize_optional(expose.catalog_id);
    let usage = expose.usage.map(|usage| ServiceExposeUsage {
        kind: usage.kind,
        path: normalize_optional(usage.path),
    });

    Ok(Some(ServiceExpose {
        transport: ServiceExposeTransport {
            kind: transport.kind,
        },
        usage,
        icon_url,
        catalog_id,
    }))
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

fn validate_manifest_exposed_ports(
    expose: Option<&ServiceExpose>,
    ports: &[ServicePort],
) -> Result<()> {
    let Some(expose) = expose else {
        return Ok(());
    };

    if expose.transport.kind != ServiceExposeTransportKind::Tcp {
        return Ok(());
    }

    if ports.iter().any(|port| {
        port.protocol == ServicePortProtocol::Tcp
            && port.name.as_ref().is_some_and(|name| !name.is_empty())
    }) {
        return Ok(());
    }

    bail!(
        "spec.expose.enabled=true with tcp transport requires at least one named TCP port in spec.ports[].name"
    )
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

fn resolve_manifest_host_port(
    host_port: Option<ServiceManifestHostPort>,
    protocol: ServicePortProtocol,
    reserved_host_ports: &mut BTreeSet<u16>,
) -> Result<ResolvedManifestHostPort> {
    match host_port {
        Some(ServiceManifestHostPort::Fixed(port)) => {
            if !reserved_host_ports.insert(port) {
                bail!("host port is already reserved in this manifest or node: {port}");
            }
            Ok(ResolvedManifestHostPort {
                port,
                allocation: ServicePortAllocation::Fixed,
            })
        }
        Some(ServiceManifestHostPort::Keyword(value)) => {
            let keyword = value.trim().to_ascii_lowercase();
            if keyword != "auto" {
                bail!("hostPort must be a number or the keyword: auto");
            }
            allocate_auto_host_port(protocol, reserved_host_ports)
        }
        None => allocate_auto_host_port(protocol, reserved_host_ports),
    }
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

fn resolve_manifest_path(
    path: &str,
    base_dir: &Path,
    fungi_home: &Path,
    service_data_dir: &Path,
) -> PathBuf {
    let expanded = resolve_manifest_path_string(path, base_dir, fungi_home, service_data_dir);
    PathBuf::from(expanded)
}

fn resolve_manifest_path_string(
    path: &str,
    base_dir: &Path,
    fungi_home: &Path,
    service_data_dir: &Path,
) -> String {
    let fungi_home_value = fungi_home.to_string_lossy();
    let service_data_value = service_data_dir.to_string_lossy();
    let expanded = path
        .replace("${FUNGI_HOME}", &fungi_home_value)
        .replace("$FUNGI_HOME", &fungi_home_value)
        .replace("${fungi_home}", &fungi_home_value)
        .replace("$fungi_home", &fungi_home_value)
        .replace("${SERVICE_DATA}", &service_data_value)
        .replace("$SERVICE_DATA", &service_data_value)
        .replace("${service_data}", &service_data_value)
        .replace("$service_data", &service_data_value)
        .replace("${APP_HOME}", &service_data_value)
        .replace("$APP_HOME", &service_data_value)
        .replace("${app_home}", &service_data_value)
        .replace("$app_home", &service_data_value);
    let resolved = PathBuf::from(&expanded);
    if resolved.is_absolute() {
        resolved.to_string_lossy().to_string()
    } else {
        base_dir.join(resolved).to_string_lossy().to_string()
    }
}
