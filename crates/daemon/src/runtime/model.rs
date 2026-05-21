use std::{collections::BTreeMap, fmt, path::PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeKind {
    Docker,
    Wasmtime,
    External,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceManifest {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition_id: Option<String>,
    pub runtime: RuntimeKind,
    #[serde(default)]
    pub run_mode: ServiceRunMode,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ServiceRunMode {
    #[default]
    Command,
    Http,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServiceSource {
    Docker { image: String },
    WasmtimeFile { component: PathBuf },
    WasmtimeUrl { url: String },
    ExistingTcp { host: String, port: u16 },
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition_id: Option<String>,
    pub source: String,
    pub labels: BTreeMap<String, String>,
    #[serde(default)]
    pub ports: Vec<ServicePort>,
    #[serde(default)]
    pub exposed_endpoints: Vec<ServiceExposeEndpointBinding>,
    pub status: ServiceStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServicePhase {
    Running,
    Stopped,
    Exited,
    Missing,
    Unknown,
}

impl ServicePhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Stopped => "stopped",
            Self::Exited => "exited",
            Self::Missing => "missing",
            Self::Unknown => "unknown",
        }
    }
}

impl fmt::Display for ServicePhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceStatus {
    pub phase: ServicePhase,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
}

impl ServiceStatus {
    pub fn running() -> Self {
        Self::new(ServicePhase::Running)
    }

    pub fn stopped() -> Self {
        Self::new(ServicePhase::Stopped)
    }

    pub fn exited(exit_code: Option<i32>) -> Self {
        Self {
            exit_code,
            ..Self::new(ServicePhase::Exited)
        }
    }

    pub fn missing() -> Self {
        Self::new(ServicePhase::Missing)
    }

    pub fn unknown() -> Self {
        Self::new(ServicePhase::Unknown)
    }

    pub fn new(phase: ServicePhase) -> Self {
        Self {
            phase,
            runtime_state: None,
            exit_code: None,
        }
    }

    pub fn with_runtime_state(mut self, runtime_state: impl Into<String>) -> Self {
        self.runtime_state = Some(runtime_state.into());
        self
    }

    pub fn is_running(&self) -> bool {
        self.phase == ServicePhase::Running
    }
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
    #[serde(default)]
    pub host_port: u16,
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
