use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeKind {
    Docker,
    Wasmtime,
    Link,
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
    TcpLink { host: String, port: u16 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceExpose {
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
    #[serde(default)]
    pub host_port_allocation: ServicePortAllocation,
    pub service_port: u16,
    pub protocol: ServicePortProtocol,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServicePortAllocation {
    Auto,
    Fixed,
}

impl Default for ServicePortAllocation {
    fn default() -> Self {
        Self::Fixed
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServicePortProtocol {
    Tcp,
    Udp,
}

#[derive(Debug, Clone, Default)]
pub struct ManifestResolutionPolicy;

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
pub struct CatalogService {
    pub service_name: String,
    pub runtime: RuntimeKind,
    pub transport: ServiceExposeTransport,
    pub usage: Option<ServiceExposeUsage>,
    pub icon_url: Option<String>,
    pub catalog_id: Option<String>,
    #[serde(default)]
    pub endpoints: Vec<CatalogServiceEndpoint>,
    pub status: ServiceStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogServiceEndpoint {
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
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run: Option<ServiceManifestRun>,
    #[serde(default)]
    pub entries: BTreeMap<String, ServiceManifestEntry>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub mounts: Vec<ServiceManifestMount>,
    #[serde(default)]
    pub command: Vec<String>,
    #[serde(default)]
    pub entrypoint: Vec<String>,
    #[serde(rename = "workingDir")]
    pub working_dir: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServiceManifestRun {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docker: Option<ServiceManifestDockerRun>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wasmtime: Option<ServiceManifestWasmtimeRun>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceManifestDockerRun {
    pub image: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServiceManifestWasmtimeRun {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServiceManifestEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<ServicePortProtocol>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<ServiceManifestEntryUsageKind>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(rename = "catalogId")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
    #[serde(rename = "iconUrl")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catalog_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceManifestEntryUsageKind {
    Web,
    Ssh,
    Tcp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceManifestMount {
    #[serde(rename = "hostPath")]
    pub host_path: String,
    #[serde(rename = "runtimePath")]
    pub runtime_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceManifestExpose {
    #[serde(default)]
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transport: Option<ServiceManifestExposeTransport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<ServiceManifestExposeUsage>,
    #[serde(rename = "catalogId")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catalog_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceManifestExposeTransport {
    pub kind: ServiceExposeTransportKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceManifestExposeUsage {
    pub kind: ServiceExposeUsageKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}
