use serde::{Deserialize, Serialize};

use crate::ServiceApplyOutcome;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ServiceControlRequest {
    PullService {
        request_id: Option<String>,
        manifest_yaml: String,
    },
    ListServices {
        request_id: Option<String>,
    },
    GetServiceLogs {
        request_id: Option<String>,
        service: String,
        tail: usize,
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
            Self::PullService { request_id, .. }
            | Self::ListServices { request_id, .. }
            | Self::GetServiceLogs { request_id, .. }
            | Self::StartService { request_id, .. }
            | Self::StopService { request_id, .. }
            | Self::RemoveService { request_id, .. } => request_id.as_deref(),
        }
    }

    pub fn service_name(&self) -> Option<String> {
        match self {
            Self::PullService { .. } => None,
            Self::ListServices { .. } => None,
            Self::GetServiceLogs { service, .. }
            | Self::StartService { service, .. }
            | Self::StopService { service, .. }
            | Self::RemoveService { service, .. } => Some(service.clone()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceControlResponse {
    pub request_id: Option<String>,
    pub ok: bool,
    #[serde(default)]
    pub forgotten_locally: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service: Option<ServiceControlServiceRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub services_json: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logs_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub apply_outcome: Option<ServiceApplyOutcome>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ServiceControlError>,
}

impl ServiceControlResponse {
    pub fn success(request_id: Option<String>, service_name: String) -> Self {
        Self {
            request_id,
            ok: true,
            forgotten_locally: false,
            service: Some(ServiceControlServiceRef { name: service_name }),
            services_json: None,
            logs_text: None,
            apply_outcome: None,
            error: None,
        }
    }

    pub fn applied(
        request_id: Option<String>,
        service_name: String,
        apply_outcome: ServiceApplyOutcome,
    ) -> Self {
        let error = apply_outcome
            .failure_summary()
            .map(|message| ServiceControlError {
                code: "partial_apply".to_string(),
                message,
            });
        Self {
            request_id,
            ok: error.is_none(),
            forgotten_locally: false,
            service: Some(ServiceControlServiceRef { name: service_name }),
            services_json: None,
            logs_text: None,
            apply_outcome: Some(apply_outcome),
            error,
        }
    }

    pub fn success_forgotten_locally(request_id: Option<String>, service_name: String) -> Self {
        Self {
            request_id,
            ok: true,
            forgotten_locally: true,
            service: Some(ServiceControlServiceRef { name: service_name }),
            services_json: None,
            logs_text: None,
            apply_outcome: None,
            error: None,
        }
    }

    pub fn success_services(request_id: Option<String>, services_json: String) -> Self {
        Self {
            request_id,
            ok: true,
            forgotten_locally: false,
            service: None,
            services_json: Some(services_json),
            logs_text: None,
            apply_outcome: None,
            error: None,
        }
    }

    pub fn success_logs(request_id: Option<String>, service_name: String, text: String) -> Self {
        Self {
            request_id,
            ok: true,
            forgotten_locally: false,
            service: Some(ServiceControlServiceRef { name: service_name }),
            services_json: None,
            logs_text: Some(text),
            apply_outcome: None,
            error: None,
        }
    }

    pub fn error(request_id: Option<String>, code: &str, message: String) -> Self {
        Self {
            request_id,
            ok: false,
            forgotten_locally: false,
            service: None,
            services_json: None,
            logs_text: None,
            apply_outcome: None,
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

    pub fn into_apply_result(self) -> anyhow::Result<Self> {
        if self.ok
            || self
                .apply_outcome
                .as_ref()
                .is_some_and(|outcome| outcome.failure.is_some())
        {
            Ok(self)
        } else {
            self.into_result()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ServiceManifestChange, ServicePhase, ServiceStatus, ServiceWorkloadAction};

    #[test]
    fn legacy_response_without_apply_outcome_remains_compatible() {
        let response: ServiceControlResponse =
            serde_json::from_str(r#"{"request_id":null,"ok":true,"service":{"name":"demo"}}"#)
                .unwrap();

        assert!(response.apply_outcome.is_none());
    }

    #[test]
    fn applied_response_round_trips_structured_outcome() {
        let response = ServiceControlResponse::applied(
            None,
            "demo".to_string(),
            ServiceApplyOutcome {
                manifest_change: ServiceManifestChange::Unchanged,
                workload_action: ServiceWorkloadAction::Restarted,
                final_status: ServiceStatus::new(ServicePhase::Running),
                failure: None,
            },
        );

        let encoded = serde_json::to_string(&response).unwrap();
        let decoded: ServiceControlResponse = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded.apply_outcome, response.apply_outcome);
    }

    #[test]
    fn partial_apply_response_preserves_remote_failure_details() {
        let response = ServiceControlResponse::applied(
            None,
            "demo".to_string(),
            ServiceApplyOutcome {
                manifest_change: ServiceManifestChange::Changed,
                workload_action: ServiceWorkloadAction::None,
                final_status: ServiceStatus::stopped(),
                failure: Some(crate::ServiceApplyFailure {
                    stage: crate::ServiceApplyFailureStage::Restart,
                    message: "launcher failed".to_string(),
                }),
            },
        );

        assert!(!response.ok);
        assert_eq!(response.error.as_ref().unwrap().code, "partial_apply");

        let encoded = serde_json::to_string(&response).unwrap();
        let decoded: ServiceControlResponse = serde_json::from_str(&encoded).unwrap();
        let preserved = decoded.into_apply_result().unwrap();
        assert_eq!(
            preserved.apply_outcome.unwrap().failure.unwrap().message,
            "launcher failed"
        );
    }
}
