use std::{collections::BTreeMap, fmt, path::PathBuf, time::SystemTime};

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
    pub detail: Option<String>,
}

impl ServiceStatus {
    pub fn running() -> Self {
        Self::new(ServicePhase::Running)
    }

    pub fn stopped() -> Self {
        Self::new(ServicePhase::Stopped)
    }

    pub fn exited(exit_code: Option<i32>) -> Self {
        let detail = exit_code.map(|code| format!("exited({code})"));
        Self::new(ServicePhase::Exited).with_optional_detail(detail)
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
            detail: None,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn with_optional_detail(mut self, detail: Option<String>) -> Self {
        self.detail = detail;
        self
    }

    pub fn is_running(&self) -> bool {
        self.phase == ServicePhase::Running
    }

    pub fn state_label(&self) -> String {
        self.detail
            .clone()
            .unwrap_or_else(|| self.phase.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{ServicePhase, ServiceStatus};

    #[test]
    fn status_label_prefers_detail() {
        let status = ServiceStatus::new(ServicePhase::Unknown).with_detail("paused");

        assert_eq!(status.state_label(), "paused");
    }

    #[test]
    fn exited_status_formats_exit_code_as_detail() {
        let status = ServiceStatus::exited(Some(137));

        assert_eq!(status.phase, ServicePhase::Exited);
        assert_eq!(status.detail.as_deref(), Some("exited(137)"));
        assert_eq!(status.state_label(), "exited(137)");
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceServiceSnapshot {
    pub peer_id: String,
    pub services: Vec<DeviceService>,
    pub updated_at: SystemTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceService {
    pub name: String,
    pub runtime: RuntimeKind,
    #[serde(default)]
    pub metadata: DeviceServiceMetadata,
    #[serde(default)]
    pub endpoints: Vec<DeviceServiceEndpoint>,
    pub status: ServiceStatus,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeviceServiceMetadata {
    pub usage: Option<ServiceExposeUsage>,
    pub icon_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceServiceEndpoint {
    pub name: String,
    pub protocol: String,
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
