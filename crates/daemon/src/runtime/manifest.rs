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
        if self.kind != "ServiceManifest" {
            bail!("Unsupported manifest kind: {}", self.kind);
        }

        let ServiceManifestDocument {
            api_version: _,
            kind: _,
            metadata,
            spec,
        } = self;
        let service_name = metadata.name;
        let metadata_labels = metadata.labels;
        let app_home = fungi_home.join("services").join(&service_name);
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
                    component: resolve_manifest_path(&file, base_dir, fungi_home, &app_home),
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
        };

        let ports = spec
            .ports
            .into_iter()
            .map(|port| {
                Ok(ServicePort {
                    name: normalize_optional(port.name),
                    host_port: resolve_manifest_host_port(
                        port.host_port,
                        port.protocol,
                        policy,
                        &mut reserved_host_ports,
                    )?,
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
                        &app_home,
                    ),
                    runtime_path: mount.runtime_path,
                })
                .collect(),
            ports,
            command: spec.command,
            entrypoint: spec.entrypoint,
            working_dir: spec
                .working_dir
                .map(|value| resolve_manifest_path_string(&value, base_dir, fungi_home, &app_home)),
            labels: metadata_labels,
        })
    }
}

fn parse_manifest_expose(
    expose: Option<ServiceManifestExpose>,
    manifest_name: &str,
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
    let service_id = normalize_non_empty(
        expose.service_id.as_deref().unwrap_or(manifest_name),
        "spec.expose.serviceId",
    )?;
    let display_name = normalize_non_empty(
        expose.display_name.as_deref().unwrap_or(manifest_name),
        "spec.expose.displayName",
    )?;
    let icon_url = normalize_optional(expose.icon_url);
    let catalog_id = normalize_optional(expose.catalog_id);
    let usage = expose.usage.map(|usage| ServiceExposeUsage {
        kind: usage.kind,
        path: normalize_optional(usage.path),
    });

    Ok(Some(ServiceExpose {
        service_id,
        display_name,
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
                protocol: service_port_protocol(&expose.service_id, name),
                host_port: port.host_port,
                service_port: port.service_port,
            })
        })
        .collect::<Vec<_>>();

    endpoints.sort_by(|left, right| left.name.cmp(&right.name));
    endpoints
}

fn resolve_manifest_host_port(
    host_port: ServiceManifestHostPort,
    protocol: ServicePortProtocol,
    policy: &ManifestResolutionPolicy,
    reserved_host_ports: &mut BTreeSet<u16>,
) -> Result<u16> {
    match host_port {
        ServiceManifestHostPort::Fixed(port) => {
            if !reserved_host_ports.insert(port) {
                bail!("host port is already reserved in this manifest or node: {port}");
            }
            Ok(port)
        }
        ServiceManifestHostPort::Keyword(value) => {
            let keyword = value.trim().to_ascii_lowercase();
            if keyword != "auto" {
                bail!("hostPort must be a number or the keyword: auto");
            }

            for port in iter_allowed_tcp_ports(policy) {
                if reserved_host_ports.contains(&port) {
                    continue;
                }
                if !is_host_port_available(port, protocol) {
                    continue;
                }
                reserved_host_ports.insert(port);
                return Ok(port);
            }

            bail!("failed to allocate host port automatically from allowed port policy")
        }
    }
}

fn iter_allowed_tcp_ports(policy: &ManifestResolutionPolicy) -> Vec<u16> {
    let mut ports = BTreeSet::new();
    for port in &policy.allowed_tcp_ports {
        ports.insert(*port);
    }
    for range in &policy.allowed_tcp_port_ranges {
        for port in range.start..=range.end {
            ports.insert(port);
        }
    }
    ports.into_iter().collect()
}

fn is_host_port_available(port: u16, protocol: ServicePortProtocol) -> bool {
    match protocol {
        ServicePortProtocol::Tcp => StdTcpListener::bind(("0.0.0.0", port)).is_ok(),
        ServicePortProtocol::Udp => StdUdpSocket::bind(("0.0.0.0", port)).is_ok(),
    }
}

fn resolve_manifest_path(
    path: &str,
    base_dir: &Path,
    fungi_home: &Path,
    app_home: &Path,
) -> PathBuf {
    let expanded = resolve_manifest_path_string(path, base_dir, fungi_home, app_home);
    PathBuf::from(expanded)
}

fn resolve_manifest_path_string(
    path: &str,
    base_dir: &Path,
    fungi_home: &Path,
    app_home: &Path,
) -> String {
    let fungi_home_value = fungi_home.to_string_lossy();
    let app_home_value = app_home.to_string_lossy();
    let expanded = path
        .replace("${FUNGI_HOME}", &fungi_home_value)
        .replace("$FUNGI_HOME", &fungi_home_value)
        .replace("${fungi_home}", &fungi_home_value)
        .replace("$fungi_home", &fungi_home_value)
        .replace("${APP_HOME}", &app_home_value)
        .replace("$APP_HOME", &app_home_value)
        .replace("${app_home}", &app_home_value)
        .replace("$app_home", &app_home_value);
    let resolved = PathBuf::from(&expanded);
    if resolved.is_absolute() {
        resolved.to_string_lossy().to_string()
    } else {
        base_dir.join(resolved).to_string_lossy().to_string()
    }
}
