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
    if let Some(document) = parse_fungi_service_document(content)? {
        return document.into_service_manifest_for_node(
            base_dir,
            fungi_home,
            policy,
            used_host_ports,
        );
    }

    let document = parse_legacy_service_manifest_yaml(content, "service manifest YAML")?;
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
    if let Some(document) = parse_fungi_service_document(content)? {
        return document.into_service_manifest_for_node_with_service_paths(
            base_dir,
            fungi_home,
            path_roots,
            policy,
            used_host_ports,
        );
    }

    let document = parse_legacy_service_manifest_yaml(content, "service manifest YAML")?;
    document.into_service_manifest_for_node_with_service_paths(
        base_dir,
        fungi_home,
        path_roots,
        policy,
        used_host_ports,
    )
}

pub fn peek_service_manifest_name(content: &str) -> Result<String> {
    if let Some(document) = parse_fungi_service_document(content)? {
        return document.service_name();
    }

    let document = parse_legacy_service_manifest_yaml(content, "service manifest YAML")?;
    normalize_non_empty(&document.metadata.name, "metadata.name")
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

    let mut document = parse_legacy_service_manifest_yaml(content, "service manifest YAML")?;
    document.metadata.name = service_name;
    serde_yaml::to_string(&document).context("Failed to encode service manifest YAML")
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
                mode: (manifest.run_mode == ServiceRunMode::Http).then_some(ServiceRunMode::Http),
            }),
        }),
        ServiceSource::WasmtimeUrl { url } => Some(ServiceManifestRun {
            docker: None,
            wasmtime: Some(ServiceManifestWasmtimeRun {
                file: None,
                url: Some(url.clone()),
                mode: (manifest.run_mode == ServiceRunMode::Http).then_some(ServiceRunMode::Http),
            }),
        }),
        ServiceSource::ExistingTcp { .. } => None,
    };
    let entries = manifest_entries_to_document(manifest);

    let document = ServiceManifestDocument {
        api_version: "fungi.rs/v1alpha1".to_string(),
        kind: "Service".to_string(),
        metadata: ServiceManifestMetadata {
            name: manifest.name.clone(),
            definition_id: manifest.definition_id.clone(),
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
        let definition_id = metadata
            .definition_id
            .map(|value| normalize_non_empty(&value, "metadata.definitionId"))
            .transpose()?;
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
            definition_id,
            runtime: runtime_and_source.runtime,
            run_mode: runtime_and_source.run_mode,
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
    kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    path: Option<String>,
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

fn parse_legacy_service_manifest_yaml(yaml: &str, label: &str) -> Result<ServiceManifestDocument> {
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
            fungi_home,
            &path_roots,
            policy,
            used_host_ports,
        )
    }

    fn into_service_manifest_for_node_with_service_paths(
        self,
        base_dir: &Path,
        fungi_home: &Path,
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
                let runtime_and_source =
                    parse_fungi_run(&run, &publish, base_dir, fungi_home, path_roots)?;
                let env = run.env;
                let mounts = run
                    .mounts
                    .into_iter()
                    .map(|mount| ServiceMount {
                        host_path: resolve_manifest_path(
                            &mount.from,
                            base_dir,
                            fungi_home,
                            path_roots,
                        ),
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
    fungi_home: &Path,
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
                    component: resolve_manifest_path(&file, base_dir, fungi_home, path_roots),
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
        icon_url: None,
        catalog_id: None,
    }))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FungiClientExposeMetadata {
    usage: Option<ServiceExposeUsageKind>,
    path: Option<String>,
}

fn fungi_client_expose_metadata(entry: &FungiServicePublishEntry) -> FungiClientExposeMetadata {
    let usage = entry.client.as_ref().map(|client| {
        let kind = client.kind.trim().to_ascii_lowercase();
        match kind.as_str() {
            "web" => ServiceExposeUsageKind::Web,
            "ssh" => ServiceExposeUsageKind::Ssh,
            _ => ServiceExposeUsageKind::Raw,
        }
    });
    let path = entry.client.as_ref().and_then(|client| {
        (usage == Some(ServiceExposeUsageKind::Web))
            .then(|| normalize_optional(client.path.clone()))
            .flatten()
    });
    FungiClientExposeMetadata { usage, path }
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
            ServiceSource::ExistingTcp { host, port } => ServiceManifestEntry {
                target: Some(format!("{host}:{port}")),
                port: None,
                host_port: None,
                protocol,
                usage,
                path: path.clone(),
                icon_url: icon_url.clone(),
                catalog_id: catalog_id.clone(),
            },
            _ => ServiceManifestEntry {
                target: None,
                port: Some(port.service_port),
                host_port: Some(port.host_port),
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
        None => parse_existing_tcp_run(entries),
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
                run_mode: ServiceRunMode::Command,
                source: ServiceSource::Docker { image },
            })
        }
        (None, Some(wasmtime)) => {
            let ServiceManifestWasmtimeRun { file, url, mode } = wasmtime;
            match (file, url) {
                (Some(file), None) => Ok(RuntimeAndSource {
                    runtime: RuntimeKind::Wasmtime,
                    run_mode: mode.unwrap_or_default(),
                    source: ServiceSource::WasmtimeFile {
                        component: resolve_manifest_path(&file, base_dir, fungi_home, path_roots),
                    },
                }),
                (None, Some(url)) => {
                    let url = normalize_non_empty(&url, "spec.run.wasmtime.url")?;
                    Ok(RuntimeAndSource {
                        runtime: RuntimeKind::Wasmtime,
                        run_mode: mode.unwrap_or_default(),
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
            }
        }
        (Some(_), Some(_)) => {
            bail!("service manifest accepts only one runtime under spec.run")
        }
        (None, None) => bail!("spec.run requires docker or wasmtime"),
    }
}

fn parse_existing_tcp_run(
    entries: &BTreeMap<String, ServiceManifestEntry>,
) -> Result<RuntimeAndSource> {
    if entries.len() != 1 {
        bail!("service manifests without spec.run currently support exactly one TCP entry");
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
        runtime: RuntimeKind::External,
        run_mode: ServiceRunMode::Command,
        source: ServiceSource::ExistingTcp { host, port },
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

    match (entry.target.as_deref(), entry.port, entry.host_port) {
        (Some(_), Some(_), _) => {
            bail!("spec.entries.{name} must use either target or port, not both");
        }
        (Some(_), None, Some(_)) => {
            bail!("spec.entries.{name}.hostPort cannot be used with target");
        }
        (Some(target), None, None) => {
            if runtime != RuntimeKind::External {
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
        (None, Some(service_port), host_port) => {
            if runtime == RuntimeKind::External {
                bail!("spec.entries.{name}.port cannot be used without spec.run");
            }
            if service_port == 0 {
                bail!("spec.entries.{name}.port must be greater than 0");
            }
            let resolved_port = match host_port {
                Some(host_port) => {
                    if host_port == 0 {
                        bail!("spec.entries.{name}.hostPort must be greater than 0");
                    }
                    if !reserved_host_ports.insert(host_port) {
                        bail!("spec.entries.{name}.hostPort {host_port} is already reserved");
                    }
                    ResolvedManifestHostPort {
                        port: host_port,
                        allocation: ServicePortAllocation::Fixed,
                    }
                }
                None => allocate_auto_host_port(protocol, reserved_host_ports)?,
            };
            Ok(ServicePort {
                name: Some(name),
                host_port: resolved_port.port,
                host_port_allocation: resolved_port.allocation,
                service_port,
                protocol,
            })
        }
        (None, None, Some(_)) => bail!("spec.entries.{name}.hostPort requires port"),
        (None, None, None) => bail!("spec.entries.{name} requires target or port"),
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
        .replace("$fungi.service.artifacts", &service_artifacts_value)
        .replace("$fungi.service.data", &service_appdata_value)
        .replace("$fungi.workspace", &user_home_value)
        .replace("$fungi.root", &user_root_value)
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
