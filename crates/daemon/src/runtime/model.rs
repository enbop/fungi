use std::{collections::BTreeMap, path::PathBuf};

use fungi_config::runtime::AllowedPortRange;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeKind {
    Docker,
    Wasmtime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceManifest {
    pub name: String,
    pub runtime: RuntimeKind,
    pub source: ServiceSource,
    pub expose: Option<ServiceExpose>,
    pub env: BTreeMap<String, String>,
    pub mounts: Vec<ServiceMount>,
    pub ports: Vec<ServicePort>,
    pub command: Vec<String>,
    pub entrypoint: Vec<String>,
    pub working_dir: Option<String>,
    pub labels: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServiceSource {
    Docker { image: String },
    WasmtimeFile { component: PathBuf },
    WasmtimeUrl { url: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceExpose {
    pub service_id: String,
    pub display_name: String,
    pub transport: ServiceExposeTransport,
    pub usage: Option<ServiceExposeUsage>,
    pub icon_url: Option<String>,
    pub catalog_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceExposeTransport {
    pub kind: ServiceExposeTransportKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceExposeTransportKind {
    Tcp,
    Raw,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceExposeUsage {
    pub kind: ServiceExposeUsageKind,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceExposeUsageKind {
    Web,
    Ssh,
    Raw,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceMount {
    pub host_path: PathBuf,
    pub runtime_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServicePort {
    pub name: Option<String>,
    pub host_port: u16,
    pub service_port: u16,
    pub protocol: ServicePortProtocol,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServicePortProtocol {
    Tcp,
    Udp,
}

#[derive(Debug, Clone, Default)]
pub struct ManifestResolutionPolicy {
    pub allowed_tcp_ports: Vec<u16>,
    pub allowed_tcp_port_ranges: Vec<AllowedPortRange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInstance {
    #[serde(default)]
    pub id: String,
    pub runtime: RuntimeKind,
    pub name: String,
    pub source: String,
    pub labels: BTreeMap<String, String>,
    #[serde(default)]
    pub ports: Vec<ServicePort>,
    #[serde(default)]
    pub exposed_endpoints: Vec<ServiceExposeEndpointBinding>,
    pub status: ServiceStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceStatus {
    pub state: String,
    pub running: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredService {
    pub service_name: String,
    pub service_id: String,
    pub display_name: String,
    pub runtime: RuntimeKind,
    pub transport: ServiceExposeTransport,
    pub usage: Option<ServiceExposeUsage>,
    pub icon_url: Option<String>,
    pub catalog_id: Option<String>,
    #[serde(default)]
    pub endpoints: Vec<DiscoveredServiceEndpoint>,
    pub status: ServiceStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredServiceEndpoint {
    pub name: String,
    pub protocol: String,
    pub service_port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceExposeEndpointBinding {
    pub name: String,
    pub protocol: String,
    pub host_port: u16,
    pub service_port: u16,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServiceLogsOptions {
    pub tail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceLogs {
    pub raw: Vec<u8>,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceManifestDocument {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    pub metadata: ServiceManifestMetadata,
    pub spec: ServiceManifestSpec,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServiceManifestMetadata {
    pub name: String,
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceManifestSpec {
    pub runtime: RuntimeKind,
    pub source: ServiceManifestSource,
    pub expose: Option<ServiceManifestExpose>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub mounts: Vec<ServiceManifestMount>,
    #[serde(default)]
    pub ports: Vec<ServiceManifestPort>,
    #[serde(default)]
    pub command: Vec<String>,
    #[serde(default)]
    pub entrypoint: Vec<String>,
    #[serde(rename = "workingDir")]
    pub working_dir: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServiceManifestSource {
    pub image: Option<String>,
    pub file: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceManifestMount {
    #[serde(rename = "hostPath")]
    pub host_path: String,
    #[serde(rename = "runtimePath")]
    pub runtime_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceManifestPort {
    #[serde(rename = "hostPort")]
    pub host_port: ServiceManifestHostPort,
    #[serde(rename = "servicePort")]
    pub service_port: u16,
    #[serde(default)]
    pub name: Option<String>,
    pub protocol: ServicePortProtocol,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ServiceManifestHostPort {
    Fixed(u16),
    Keyword(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceManifestExpose {
    #[serde(default)]
    pub enabled: bool,
    #[serde(rename = "serviceId")]
    pub service_id: Option<String>,
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
    pub transport: Option<ServiceManifestExposeTransport>,
    pub usage: Option<ServiceManifestExposeUsage>,
    #[serde(rename = "iconUrl")]
    pub icon_url: Option<String>,
    #[serde(rename = "catalogId")]
    pub catalog_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceManifestExposeTransport {
    pub kind: ServiceExposeTransportKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceManifestExposeUsage {
    pub kind: ServiceExposeUsageKind,
    pub path: Option<String>,
}
