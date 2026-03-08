use serde::{Deserialize, Serialize};

use crate::ServiceManifest;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ServiceControlRequest {
    DeployService {
        request_id: Option<String>,
        manifest: ServiceManifest,
    },
    StartService {
        request_id: Option<String>,
        service: String,
    },
    StopService {
        request_id: Option<String>,
        service: String,
    },
    RemoveService {
        request_id: Option<String>,
        service: String,
    },
}

impl ServiceControlRequest {
    pub fn request_id(&self) -> Option<&str> {
        match self {
            Self::DeployService { request_id, .. }
            | Self::StartService { request_id, .. }
            | Self::StopService { request_id, .. }
            | Self::RemoveService { request_id, .. } => request_id.as_deref(),
        }
    }

    pub fn service_name(&self) -> String {
        match self {
            Self::DeployService { manifest, .. } => manifest.name.clone(),
            Self::StartService { service, .. }
            | Self::StopService { service, .. }
            | Self::RemoveService { service, .. } => service.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceControlResponse {
    pub request_id: Option<String>,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service: Option<ServiceControlServiceRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ServiceControlError>,
}

impl ServiceControlResponse {
    pub fn success(request_id: Option<String>, service_name: String) -> Self {
        Self {
            request_id,
            ok: true,
            service: Some(ServiceControlServiceRef { name: service_name }),
            error: None,
        }
    }

    pub fn error(request_id: Option<String>, code: &str, message: String) -> Self {
        Self {
            request_id,
            ok: false,
            service: None,
            error: Some(ServiceControlError {
                code: code.to_string(),
                message,
            }),
        }
    }

    pub fn into_result(self) -> anyhow::Result<Self> {
        if self.ok {
            Ok(self)
        } else {
            let error = self.error.unwrap_or(ServiceControlError {
                code: "remote_error".to_string(),
                message: "remote service control failed".to_string(),
            });
            anyhow::bail!("{}: {}", error.code, error.message)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceControlServiceRef {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceControlError {
    pub code: String,
    pub message: String,
}