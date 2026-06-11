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
    let document = parse_required_fungi_service_document(content)?;
    document.into_service_manifest_for_node(base_dir, fungi_home, policy, used_host_ports)
}

pub(crate) fn parse_service_manifest_yaml_with_policy_for_service_paths(
    content: &str,
    base_dir: &Path,
    path_roots: &ManifestPathRoots,
    policy: &ManifestResolutionPolicy,
    used_host_ports: &BTreeSet<u16>,
) -> Result<ServiceManifest> {
    let document = parse_required_fungi_service_document(content)?;
    document.into_service_manifest_for_node_with_service_paths(
        base_dir,
        path_roots,
        policy,
        used_host_ports,
    )
}

pub fn peek_service_manifest_name(content: &str) -> Result<String> {
    parse_required_fungi_service_document(content)?.service_name()
}

pub fn service_manifest_with_instance_name(content: &str, service_name: &str) -> Result<String> {
    let service_name = normalize_non_empty(service_name, "service name")?;
    if let Some(front_matter) = split_front_matter(content)? {
        let mut document =
            parse_fungi_service_yaml(front_matter.yaml, "Fungi service front matter")?;
        document.set_instance_name(service_name.to_string());
        let yaml = serde_yaml::to_string(&document)
            .context("Failed to encode Fungi service front matter")?;
        let yaml = yaml_without_document_start(yaml);
        return Ok(format!("---\n{}---\n{}", yaml, front_matter.body));
    }

    if should_parse_as_fungi_service_yaml(content) {
        let mut document = parse_fungi_service_yaml(content, "Fungi service YAML")?;
        document.set_instance_name(service_name.to_string());
        return serde_yaml::to_string(&document).context("Failed to encode Fungi service YAML");
    }

    bail!("service manifest must use fungi: service/v1")
}

fn yaml_without_document_start(yaml: String) -> String {
    let mut yaml = yaml
        .strip_prefix("---\n")
        .or_else(|| yaml.strip_prefix("---\r\n"))
        .unwrap_or(&yaml)
        .to_string();
    if !yaml.ends_with('\n') {
        yaml.push('\n');
    }
    yaml
}

pub fn service_manifest_to_yaml(manifest: &ServiceManifest) -> Result<String> {
    let run = match &manifest.source {
        ServiceSource::Docker { image } => Some(FungiServiceRun {
            provider: FungiServiceProvider::Docker,
            mode: None,
            source: FungiServiceSource {
                image: Some(image.clone()),
                ..FungiServiceSource::default()
            },
            args: manifest.command.clone(),
            env: manifest.env.clone(),
            mounts: manifest_mounts_to_fungi(&manifest.mounts),
        }),
        ServiceSource::WasmtimeFile { component } => Some(FungiServiceRun {
            provider: FungiServiceProvider::Wasmtime,
            mode: (manifest.run_mode == ServiceRunMode::Http).then_some(FungiServiceRunMode::Http),
            source: FungiServiceSource {
                file: Some(component.display().to_string()),
                ..FungiServiceSource::default()
            },
            args: manifest.command.clone(),
            env: manifest.env.clone(),
            mounts: manifest_mounts_to_fungi(&manifest.mounts),
        }),
        ServiceSource::WasmtimeUrl { url } => Some(FungiServiceRun {
            provider: FungiServiceProvider::Wasmtime,
            mode: (manifest.run_mode == ServiceRunMode::Http).then_some(FungiServiceRunMode::Http),
            source: FungiServiceSource {
                url: Some(url.clone()),
                ..FungiServiceSource::default()
            },
            args: manifest.command.clone(),
            env: manifest.env.clone(),
            mounts: manifest_mounts_to_fungi(&manifest.mounts),
        }),
        ServiceSource::ExistingTcp { .. } => None,
    };

    if !manifest.entrypoint.is_empty() {
        bail!("fungi: service/v1 does not support entrypoint");
    }
    if manifest.working_dir.is_some() {
        bail!("fungi: service/v1 does not support working_dir");
    }
    if !manifest.labels.is_empty() {
        bail!("fungi: service/v1 does not support labels");
    }

    let id = manifest
        .definition_id
        .clone()
        .unwrap_or_else(|| manifest.name.clone());
    let instance = (manifest.name != id).then(|| manifest.name.clone());
    let document = FungiServiceDocument {
        fungi: "service/v1".to_string(),
        id,
        instance,
        run,
        publish: manifest_publish_to_fungi(manifest),
    };

    serde_yaml::to_string(&document).context("Failed to encode Fungi service YAML")
}

fn manifest_mounts_to_fungi(mounts: &[ServiceMount]) -> Vec<FungiServiceMount> {
    mounts
        .iter()
        .map(|mount| FungiServiceMount {
            from: mount.host_path.display().to_string(),
            to: mount.runtime_path.clone(),
        })
        .collect()
}

fn manifest_publish_to_fungi(
    manifest: &ServiceManifest,
) -> BTreeMap<String, FungiServicePublishEntry> {
    let client = manifest_client_to_fungi(manifest.expose.as_ref());
    manifest
        .ports
        .iter()
        .enumerate()
        .map(|(index, port)| {
            let fallback_name = if index == 0 {
                "main".to_string()
            } else {
                format!("main-{index}")
            };
            let name = port.name.clone().unwrap_or(fallback_name);
            let (host, tcp_port) = match &manifest.source {
                ServiceSource::ExistingTcp { host, port } => (Some(host.clone()), *port),
                _ => (None, port.service_port),
            };
            (
                name,
                FungiServicePublishEntry {
                    tcp: FungiServiceTcp {
                        host,
                        port: tcp_port,
                    },
                    client: client.clone(),
                },
            )
        })
        .collect()
}

fn manifest_client_to_fungi(expose: Option<&ServiceExpose>) -> Option<FungiServiceClient> {
    let expose = expose?;
    let usage = expose.usage.as_ref();
    let kind = usage.map(|usage| match usage.kind {
        ServiceExposeUsageKind::Web => "web".to_string(),
        ServiceExposeUsageKind::Ssh => "ssh".to_string(),
        ServiceExposeUsageKind::Raw => "raw".to_string(),
    });
    let path = usage.and_then(|usage| {
        (usage.kind == ServiceExposeUsageKind::Web)
            .then(|| usage.path.clone())
            .flatten()
    });
    let icon_url = expose.icon_url.clone();
    let catalog_id = expose.catalog_id.clone();

    if kind.is_none() && path.is_none() && icon_url.is_none() && catalog_id.is_none() {
        return None;
    }

    Some(FungiServiceClient {
        kind,
        path,
        icon_url,
        catalog_id,
    })
}

struct RuntimeAndSource {
    runtime: RuntimeKind,
    run_mode: ServiceRunMode,
    source: ServiceSource,
}

#[derive(Debug, Clone)]
struct FrontMatter<'a> {
    yaml: &'a str,
    body: &'a str,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct FungiServiceDocument {
    fungi: String,
    id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    instance: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    run: Option<FungiServiceRun>,
    publish: BTreeMap<String, FungiServicePublishEntry>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct FungiServiceRun {
    provider: FungiServiceProvider,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mode: Option<FungiServiceRunMode>,
    source: FungiServiceSource,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    args: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    env: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    mounts: Vec<FungiServiceMount>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
enum FungiServiceProvider {
    Docker,
    Wasmtime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
enum FungiServiceRunMode {
    Http,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct FungiServiceSource {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    image: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct FungiServiceMount {
    from: String,
    to: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct FungiServicePublishEntry {
    tcp: FungiServiceTcp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    client: Option<FungiServiceClient>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct FungiServiceTcp {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    host: Option<String>,
    port: u16,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct FungiServiceClient {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(rename = "iconUrl")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    icon_url: Option<String>,
    #[serde(rename = "catalogId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    catalog_id: Option<String>,
}

fn parse_fungi_service_document(content: &str) -> Result<Option<FungiServiceDocument>> {
    if let Some(front_matter) = split_front_matter(content)? {
        let document = parse_fungi_service_yaml(front_matter.yaml, "Fungi service front matter")?;
        return Ok(Some(document));
    }

    if should_parse_as_fungi_service_yaml(content) {
        let document = parse_fungi_service_yaml(content, "Fungi service YAML")?;
        return Ok(Some(document));
    }

    Ok(None)
}

fn parse_required_fungi_service_document(content: &str) -> Result<FungiServiceDocument> {
    parse_fungi_service_document(content)?
        .ok_or_else(|| anyhow::anyhow!("service manifest must use fungi: service/v1"))
}

fn split_front_matter(content: &str) -> Result<Option<FrontMatter<'_>>> {
    let Some(rest) = content
        .strip_prefix("---\n")
        .or_else(|| content.strip_prefix("---\r\n"))
    else {
        return Ok(None);
    };

    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed == "---" {
            let body = &rest[offset + line.len()..];
            return Ok(Some(FrontMatter {
                yaml: &rest[..offset],
                body,
            }));
        }
        offset += line.len();
    }

    if serde_yaml::from_str::<serde_yaml::Value>(content).is_ok() {
        return Ok(None);
    }

    bail!("Fungi service file front matter is missing closing ---")
}

fn parse_fungi_service_yaml(yaml: &str, label: &str) -> Result<FungiServiceDocument> {
    serde_yaml::from_str(yaml).map_err(|error| format_yaml_parse_error(label, error))
}

fn format_yaml_parse_error(label: &str, error: serde_yaml::Error) -> anyhow::Error {
    if let Some(location) = error.location() {
        anyhow::anyhow!(
            "Failed to parse {label} at line {}, column {}: {error}",
            location.line(),
            location.column()
        )
    } else {
        anyhow::anyhow!("Failed to parse {label}: {error}")
    }
}

fn should_parse_as_fungi_service_yaml(content: &str) -> bool {
    let trimmed = content.trim_start();
    if trimmed.starts_with("fungi:") {
        return true;
    }
    if !trimmed.starts_with("---") {
        return false;
    }

    match serde_yaml::from_str::<serde_yaml::Value>(content) {
        Ok(serde_yaml::Value::Mapping(mapping)) => {
            let fungi_key = serde_yaml::Value::String("fungi".to_string());
            mapping.contains_key(&fungi_key)
        }
        Ok(_) => false,
        Err(_) => trimmed.lines().take(32).any(|line| {
            let line = line.trim_start();
            line.starts_with("fungi:")
        }),
    }
}

impl FungiServiceDocument {
    fn definition_id(&self) -> Result<String> {
        normalize_non_empty(&self.id, "id")
    }

    fn service_name(&self) -> Result<String> {
        match self.instance.as_deref() {
            Some(instance) => normalize_non_empty(instance, "instance"),
            None => self.definition_id(),
        }
    }

    fn set_instance_name(&mut self, service_name: String) {
        self.instance = Some(service_name);
    }

    fn into_service_manifest_for_node(
        self,
        base_dir: &Path,
        fungi_home: &Path,
        policy: &ManifestResolutionPolicy,
        used_host_ports: &BTreeSet<u16>,
    ) -> Result<ServiceManifest> {
        let service_name = self.service_name()?;
        let path_roots = ManifestPathRoots::for_local_service_id(fungi_home, &service_name);
        self.into_service_manifest_for_node_with_service_paths(
            base_dir,
            &path_roots,
            policy,
            used_host_ports,
        )
    }

    fn into_service_manifest_for_node_with_service_paths(
        self,
        base_dir: &Path,
        path_roots: &ManifestPathRoots,
        _policy: &ManifestResolutionPolicy,
        used_host_ports: &BTreeSet<u16>,
    ) -> Result<ServiceManifest> {
        if self.fungi != "service/v1" {
            bail!("unsupported fungi service format: {}", self.fungi);
        }

        let definition_id = self.definition_id()?;
        let service_name = self.service_name()?;
        if self.publish.is_empty() {
            bail!("service file requires at least one publish entry");
        }

        let mut reserved_host_ports = used_host_ports.clone();
        let FungiServiceDocument {
            fungi: _,
            id: _,
            instance: _,
            run,
            publish,
        } = self;

        let (runtime_and_source, env, mounts, command) = match run {
            Some(run) => {
                let runtime_and_source = parse_fungi_run(&run, &publish, base_dir, path_roots)?;
                let env = run.env;
                let mounts = run
                    .mounts
                    .into_iter()
                    .map(|mount| ServiceMount {
                        host_path: resolve_manifest_path(&mount.from, base_dir, path_roots),
                        runtime_path: mount.to,
                    })
                    .collect();
                (runtime_and_source, env, mounts, run.args)
            }
            None => (
                parse_fungi_existing_tcp_run(&publish)?,
                BTreeMap::new(),
                Vec::new(),
                Vec::new(),
            ),
        };

        let ports = parse_fungi_publish_entries(
            &publish,
            runtime_and_source.runtime,
            &mut reserved_host_ports,
        )?;
        let expose = parse_fungi_publish_expose(&publish)?;

        Ok(ServiceManifest {
            name: service_name,
            definition_id: Some(definition_id),
            runtime: runtime_and_source.runtime,
            run_mode: runtime_and_source.run_mode,
            source: runtime_and_source.source,
            expose,
            env,
            mounts,
            ports,
            command,
            entrypoint: Vec::new(),
            working_dir: None,
            labels: BTreeMap::new(),
        })
    }
}

fn parse_fungi_run(
    run: &FungiServiceRun,
    publish: &BTreeMap<String, FungiServicePublishEntry>,
    base_dir: &Path,
    path_roots: &ManifestPathRoots,
) -> Result<RuntimeAndSource> {
    for (name, entry) in publish {
        if entry.tcp.port == 0 {
            bail!("publish.{name}.tcp.port must be greater than 0");
        }
    }

    match run.provider {
        FungiServiceProvider::Docker => {
            if run.mode.is_some() {
                bail!("run.mode is currently supported only with provider: wasmtime");
            }
            let image = exactly_one_source(&run.source, "run.source", SourceField::Image)?;
            Ok(RuntimeAndSource {
                runtime: RuntimeKind::Docker,
                run_mode: ServiceRunMode::Command,
                source: ServiceSource::Docker { image },
            })
        }
        FungiServiceProvider::Wasmtime => {
            let source = match (
                normalize_optional(run.source.file.clone()),
                normalize_optional(run.source.url.clone()),
                normalize_optional(run.source.image.clone()),
            ) {
                (Some(file), None, None) => ServiceSource::WasmtimeFile {
                    component: resolve_manifest_path(&file, base_dir, path_roots),
                },
                (None, Some(url), None) => ServiceSource::WasmtimeUrl { url },
                (None, None, Some(_)) => {
                    bail!("provider: wasmtime requires source.url or source.file, not source.image")
                }
                (None, None, None) => {
                    bail!("provider: wasmtime requires source.url or source.file")
                }
                _ => bail!("provider: wasmtime accepts exactly one of source.url or source.file"),
            };
            Ok(RuntimeAndSource {
                runtime: RuntimeKind::Wasmtime,
                run_mode: match run.mode {
                    Some(FungiServiceRunMode::Http) => ServiceRunMode::Http,
                    None => ServiceRunMode::Command,
                },
                source,
            })
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum SourceField {
    Image,
}

fn exactly_one_source(
    source: &FungiServiceSource,
    field_name: &str,
    expected: SourceField,
) -> Result<String> {
    let url = normalize_optional(source.url.clone());
    let file = normalize_optional(source.file.clone());
    let image = normalize_optional(source.image.clone());
    let source_count =
        usize::from(url.is_some()) + usize::from(file.is_some()) + usize::from(image.is_some());
    if source_count != 1 {
        bail!("{field_name} must set exactly one of url, file, or image");
    }

    match expected {
        SourceField::Image => image.ok_or_else(|| {
            anyhow::anyhow!("provider: docker requires source.image, not source.url or source.file")
        }),
    }
}

fn parse_fungi_existing_tcp_run(
    publish: &BTreeMap<String, FungiServicePublishEntry>,
) -> Result<RuntimeAndSource> {
    if publish.len() != 1 {
        bail!("service files without run currently support exactly one publish entry");
    }
    let (name, entry) = publish.iter().next().expect("publish is non-empty");
    let host = normalize_fungi_tcp_host(
        entry.tcp.host.as_deref(),
        &format!("publish.{name}.tcp.host"),
    )?;
    if !matches!(host.as_str(), "127.0.0.1" | "localhost") {
        bail!("publish.{name}.tcp.host currently supports only 127.0.0.1 or localhost");
    }
    if entry.tcp.port == 0 {
        bail!("publish.{name}.tcp.port must be greater than 0");
    }
    Ok(RuntimeAndSource {
        runtime: RuntimeKind::External,
        run_mode: ServiceRunMode::Command,
        source: ServiceSource::ExistingTcp {
            host,
            port: entry.tcp.port,
        },
    })
}

fn parse_fungi_publish_entries(
    publish: &BTreeMap<String, FungiServicePublishEntry>,
    runtime: RuntimeKind,
    reserved_host_ports: &mut BTreeSet<u16>,
) -> Result<Vec<ServicePort>> {
    publish
        .iter()
        .map(|(name, entry)| parse_fungi_publish_entry(name, entry, runtime, reserved_host_ports))
        .collect()
}

fn parse_fungi_publish_entry(
    name: &str,
    entry: &FungiServicePublishEntry,
    runtime: RuntimeKind,
    reserved_host_ports: &mut BTreeSet<u16>,
) -> Result<ServicePort> {
    let name = normalize_non_empty(name, "publish entry key")?;
    let service_port = entry.tcp.port;
    if service_port == 0 {
        bail!("publish.{name}.tcp.port must be greater than 0");
    }

    match runtime {
        RuntimeKind::Docker => {
            if entry.tcp.host.is_some() {
                bail!("publish.{name}.tcp.host is not used with provider: docker; omit it");
            }
            let resolved_port =
                allocate_auto_host_port(ServicePortProtocol::Tcp, reserved_host_ports)?;
            Ok(ServicePort {
                name: Some(name),
                host_port: resolved_port.port,
                host_port_allocation: resolved_port.allocation,
                service_port,
                protocol: ServicePortProtocol::Tcp,
            })
        }
        RuntimeKind::Wasmtime | RuntimeKind::External => {
            let host = normalize_fungi_tcp_host(
                entry.tcp.host.as_deref(),
                &format!("publish.{name}.tcp.host"),
            )?;
            if !matches!(host.as_str(), "127.0.0.1" | "localhost") {
                bail!("publish.{name}.tcp.host currently supports only 127.0.0.1 or localhost");
            }
            if !reserved_host_ports.insert(service_port) {
                bail!("publish.{name}.tcp.port {service_port} is already reserved");
            }
            Ok(ServicePort {
                name: Some(name),
                host_port: service_port,
                host_port_allocation: ServicePortAllocation::Fixed,
                service_port,
                protocol: ServicePortProtocol::Tcp,
            })
        }
    }
}

fn normalize_fungi_tcp_host(value: Option<&str>, field_name: &str) -> Result<String> {
    match value {
        Some(value) => normalize_non_empty(value, field_name),
        None => Ok("127.0.0.1".to_string()),
    }
}

fn parse_fungi_publish_expose(
    publish: &BTreeMap<String, FungiServicePublishEntry>,
) -> Result<Option<ServiceExpose>> {
    let Some((first_name, first_entry)) = publish.iter().next() else {
        return Ok(None);
    };
    let first_metadata = fungi_client_expose_metadata(first_entry);
    for (name, entry) in publish.iter().skip(1) {
        let metadata = fungi_client_expose_metadata(entry);
        if metadata != first_metadata {
            bail!(
                "publish.{name}.client metadata must match publish.{first_name}.client; per-entry client handling is not supported yet"
            );
        }
    }

    let usage = first_metadata.usage.map(|kind| ServiceExposeUsage {
        kind,
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
struct FungiClientExposeMetadata {
    usage: Option<ServiceExposeUsageKind>,
    path: Option<String>,
    icon_url: Option<String>,
    catalog_id: Option<String>,
}

fn fungi_client_expose_metadata(entry: &FungiServicePublishEntry) -> FungiClientExposeMetadata {
    let usage = entry.client.as_ref().and_then(|client| {
        normalize_optional(client.kind.clone()).map(|kind| {
            match kind.to_ascii_lowercase().as_str() {
                "web" => ServiceExposeUsageKind::Web,
                "ssh" => ServiceExposeUsageKind::Ssh,
                _ => ServiceExposeUsageKind::Raw,
            }
        })
    });
    let path = entry.client.as_ref().and_then(|client| {
        (usage == Some(ServiceExposeUsageKind::Web))
            .then(|| normalize_optional(client.path.clone()))
            .flatten()
    });
    let icon_url = entry
        .client
        .as_ref()
        .and_then(|client| normalize_optional(client.icon_url.clone()));
    let catalog_id = entry
        .client
        .as_ref()
        .and_then(|client| normalize_optional(client.catalog_id.clone()));
    FungiClientExposeMetadata {
        usage,
        path,
        icon_url,
        catalog_id,
    }
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

fn resolve_manifest_path(path: &str, base_dir: &Path, path_roots: &ManifestPathRoots) -> PathBuf {
    let expanded = resolve_manifest_path_string(path, base_dir, path_roots);
    PathBuf::from(expanded)
}

fn resolve_manifest_path_string(
    path: &str,
    base_dir: &Path,
    path_roots: &ManifestPathRoots,
) -> String {
    let service_appdata_value = path_roots.service_appdata_dir.to_string_lossy();
    let service_artifacts_value = path_roots.service_artifacts_dir.to_string_lossy();
    let user_root_value = path_roots.user_root_dir.to_string_lossy();
    let user_home_value = path_roots.user_home_dir.to_string_lossy();
    let expanded = path
        .replace("$fungi.service.artifacts", &service_artifacts_value)
        .replace("$fungi.service.data", &service_appdata_value)
        .replace("$fungi.workspace", &user_home_value)
        .replace("$fungi.root", &user_root_value);
    let resolved = PathBuf::from(&expanded);
    if resolved.is_absolute() {
        resolved.to_string_lossy().to_string()
    } else {
        base_dir.join(resolved).to_string_lossy().to_string()
    }
}
