use std::path::PathBuf;

use anyhow::Result;
use fungi_stream::IncomingStreams;
use fungi_swarm::SwarmControl;
use fungi_util::protocols::FUNGI_SERVICE_CONTROL_PROTOCOL;
use futures::StreamExt;
use futures::{AsyncRead, AsyncWrite};
use libp2p::{
    PeerId,
    futures::{AsyncReadExt, AsyncWriteExt},
};
use serde::{Serialize, de::DeserializeOwned};

use crate::controls::TcpTunnelingControl;
use crate::{
    ManifestResolutionPolicy, RuntimeControl, ServiceControlRequest, ServiceControlResponse,
    ServiceManifest,
    service_endpoints::{
        sync_applied_service_endpoint_listeners, sync_service_endpoint_listeners_by_name,
        sync_service_endpoint_listeners_for_manifest,
    },
};

const MAX_CONTROL_FRAME_LEN: usize = 2 * 1024 * 1024;
pub const DEFAULT_REMOTE_SERVICE_LOG_TAIL: usize = 200;
pub const MAX_REMOTE_SERVICE_LOG_TAIL: usize = 2_000;
const MAX_REMOTE_SERVICE_LOG_BYTES: usize = 512 * 1024;
const REMOTE_LOG_TRUNCATION_NOTICE: &str = "[fungi] remote log output truncated\n";

#[derive(Clone)]
pub struct ServiceControlProtocolControl {
    swarm_control: SwarmControl,
    fungi_home: PathBuf,
    runtime_control: RuntimeControl,
    tcp_tunneling_control: TcpTunnelingControl,
}

impl ServiceControlProtocolControl {
    pub fn new(
        swarm_control: SwarmControl,
        fungi_home: PathBuf,
        runtime_control: RuntimeControl,
        tcp_tunneling_control: TcpTunnelingControl,
    ) -> Self {
        Self {
            swarm_control,
            fungi_home,
            runtime_control,
            tcp_tunneling_control,
        }
    }

    pub fn start(&self) -> Result<()> {
        let incoming_streams = self
            .swarm_control
            .accept_incoming_streams(FUNGI_SERVICE_CONTROL_PROTOCOL)
            .map_err(anyhow::Error::from)?;
        let this = self.clone();
        tokio::spawn(async move {
            this.listen_from_incoming_streams(incoming_streams).await;
        });
        Ok(())
    }

    pub async fn pull_peer_service(
        &self,
        peer_id: PeerId,
        manifest_yaml: String,
    ) -> Result<ServiceControlResponse> {
        self.send_request_raw(
            peer_id,
            ServiceControlRequest::PullService {
                request_id: None,
                manifest_yaml,
            },
        )
        .await?
        .into_apply_result()
    }

    pub async fn start_peer_service(
        &self,
        peer_id: PeerId,
        service: String,
    ) -> Result<ServiceControlResponse> {
        self.send_request(
            peer_id,
            ServiceControlRequest::StartService {
                request_id: None,
                service,
            },
        )
        .await
    }

    pub async fn list_peer_services(&self, peer_id: PeerId) -> Result<ServiceControlResponse> {
        self.send_request(
            peer_id,
            ServiceControlRequest::ListServices { request_id: None },
        )
        .await
    }

    pub async fn get_peer_service_logs(
        &self,
        peer_id: PeerId,
        service: String,
        tail: usize,
    ) -> Result<ServiceControlResponse> {
        self.send_request(
            peer_id,
            ServiceControlRequest::GetServiceLogs {
                request_id: None,
                service,
                tail: tail.clamp(1, MAX_REMOTE_SERVICE_LOG_TAIL),
            },
        )
        .await
    }

    pub async fn stop_peer_service(
        &self,
        peer_id: PeerId,
        service: String,
    ) -> Result<ServiceControlResponse> {
        self.send_request(
            peer_id,
            ServiceControlRequest::StopService {
                request_id: None,
                service,
            },
        )
        .await
    }

    pub async fn remove_peer_service(
        &self,
        peer_id: PeerId,
        service: String,
    ) -> Result<ServiceControlResponse> {
        self.send_request(
            peer_id,
            ServiceControlRequest::RemoveService {
                request_id: None,
                service,
            },
        )
        .await
    }

    async fn send_request(
        &self,
        peer_id: PeerId,
        request: ServiceControlRequest,
    ) -> Result<ServiceControlResponse> {
        self.send_request_raw(peer_id, request).await?.into_result()
    }

    async fn send_request_raw(
        &self,
        peer_id: PeerId,
        request: ServiceControlRequest,
    ) -> Result<ServiceControlResponse> {
        let (mut stream, _handle, _connection_id) = self
            .swarm_control
            .open_stream(peer_id, FUNGI_SERVICE_CONTROL_PROTOCOL)
            .await
            .map_err(|e| {
                anyhow::anyhow!("Failed to open service-control stream to peer {peer_id}: {e}")
            })?;

        write_frame(&mut stream, &request).await.map_err(|e| {
            anyhow::anyhow!("Failed to write service-control request to peer {peer_id}: {e}")
        })?;

        read_frame::<_, ServiceControlResponse>(&mut stream)
            .await
            .map_err(|e| {
                anyhow::anyhow!("Failed to read service-control response from peer {peer_id}: {e}")
            })
    }

    async fn listen_from_incoming_streams(self, mut incoming_streams: IncomingStreams) {
        while let Some(incoming_stream) = incoming_streams.next().await {
            let peer_id = incoming_stream.peer_id;
            let mut stream = incoming_stream.stream;
            let this = self.clone();
            tokio::spawn(async move {
                let request = match read_frame::<_, ServiceControlRequest>(&mut stream).await {
                    Ok(request) => request,
                    Err(error) => {
                        log::warn!(
                            "Failed to read service-control request from peer {}: {}",
                            peer_id,
                            error
                        );
                        let _ = stream.close().await;
                        return;
                    }
                };

                let response = this.handle_request(request).await;

                if let Err(error) = write_frame(&mut stream, &response).await {
                    log::warn!(
                        "Failed to write service-control response to peer {}: {}",
                        peer_id,
                        error
                    );
                    let _ = stream.close().await;
                    return;
                }

                let _ = stream.close().await;
            });
        }
    }

    async fn handle_request(&self, request: ServiceControlRequest) -> ServiceControlResponse {
        let request_id = request.request_id().map(str::to_string);

        let result = match request {
            ServiceControlRequest::PullService { manifest_yaml, .. } => {
                let policy = self.manifest_resolution_policy();
                match self
                    .runtime_control
                    .apply_manifest_yaml(
                        &manifest_yaml,
                        &self.fungi_home,
                        &self.fungi_home,
                        &policy,
                    )
                    .await
                {
                    Ok(mut applied) => {
                        sync_applied_service_endpoint_listeners(
                            &self.runtime_control,
                            &self.tcp_tunneling_control,
                            &mut applied,
                        )
                        .await;
                        return ServiceControlResponse::applied(
                            request_id,
                            applied.instance.name,
                            applied.outcome,
                        );
                    }
                    Err(error) => Err(error),
                }
            }
            ServiceControlRequest::ListServices { .. } => {
                let services = self.runtime_control.list_services().await;
                match services {
                    Ok(services) => match serde_json::to_string(&services) {
                        Ok(services_json) => {
                            return ServiceControlResponse::success_services(
                                request_id,
                                services_json,
                            );
                        }
                        Err(error) => {
                            Err(anyhow::anyhow!("Failed to serialize service list: {error}"))
                        }
                    },
                    Err(error) => Err(error),
                }
            }
            ServiceControlRequest::GetServiceLogs { service, tail, .. } => {
                let tail = tail.clamp(1, MAX_REMOTE_SERVICE_LOG_TAIL);
                match self
                    .runtime_control
                    .logs_text_by_name_bounded(&service, tail, MAX_REMOTE_SERVICE_LOG_BYTES)
                    .await
                {
                    Ok(logs) => {
                        match bounded_service_log_response(request_id.clone(), service, logs) {
                            Ok(response) => return response,
                            Err(error) => Err(error),
                        }
                    }
                    Err(error) => Err(error),
                }
            }
            ServiceControlRequest::StartService { service, .. } => {
                match self.runtime_control.start_by_name(&service).await {
                    Ok(()) => match self
                        .sync_service_endpoint_listeners_by_name(&service, true)
                        .await
                    {
                        Ok(()) => Ok(service),
                        Err(error) => Err(error),
                    },
                    Err(error) => Err(error),
                }
            }
            ServiceControlRequest::StopService { service, .. } => {
                match self.runtime_control.stop_by_name(&service).await {
                    Ok(()) => match self
                        .sync_service_endpoint_listeners_by_name(&service, false)
                        .await
                    {
                        Ok(()) => Ok(service),
                        Err(error) => Err(error),
                    },
                    Err(error) => Err(error),
                }
            }
            ServiceControlRequest::RemoveService { service, .. } => {
                let manifest = self.runtime_control.get_service_manifest(&service);
                match self.runtime_control.remove_by_name(&service).await {
                    Ok(()) => match self
                        .sync_service_endpoint_listeners_for_manifest(manifest.as_ref(), false)
                        .await
                    {
                        Ok(()) => Ok(service),
                        Err(error) => Err(error),
                    },
                    Err(error) => Err(error),
                }
            }
        };

        match result {
            Ok(service_name) => ServiceControlResponse::success(request_id, service_name),
            Err(error) => {
                ServiceControlResponse::error(request_id, "execution_failed", error.to_string())
            }
        }
    }

    fn manifest_resolution_policy(&self) -> ManifestResolutionPolicy {
        ManifestResolutionPolicy
    }

    async fn sync_service_endpoint_listeners_by_name(
        &self,
        name: &str,
        enabled: bool,
    ) -> Result<()> {
        sync_service_endpoint_listeners_by_name(
            &self.runtime_control,
            &self.tcp_tunneling_control,
            name,
            enabled,
        )
        .await
    }

    async fn sync_service_endpoint_listeners_for_manifest(
        &self,
        manifest: Option<&ServiceManifest>,
        enabled: bool,
    ) -> Result<()> {
        sync_service_endpoint_listeners_for_manifest(&self.tcp_tunneling_control, manifest, enabled)
            .await
    }
}

fn bounded_service_log_response(
    request_id: Option<String>,
    service_name: String,
    logs: crate::runtime::BoundedLogText,
) -> Result<ServiceControlResponse> {
    let (content, truncated) = limit_remote_log_content(logs.text, logs.truncated);
    let response = ServiceControlResponse::success_logs(
        request_id.clone(),
        service_name.clone(),
        render_remote_log_text(&content, truncated),
    );
    if serialized_frame_len(&response)? <= MAX_CONTROL_FRAME_LEN {
        return Ok(response);
    }

    let mut boundaries = content
        .char_indices()
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    boundaries.push(content.len());
    let mut low = 0;
    let mut high = boundaries.len() - 1;
    while low < high {
        let middle = low + (high - low) / 2;
        let candidate = ServiceControlResponse::success_logs(
            request_id.clone(),
            service_name.clone(),
            render_remote_log_text(&content[boundaries[middle]..], true),
        );
        if serialized_frame_len(&candidate)? <= MAX_CONTROL_FRAME_LEN {
            high = middle;
        } else {
            low = middle + 1;
        }
    }

    let response = ServiceControlResponse::success_logs(
        request_id,
        service_name,
        render_remote_log_text(&content[boundaries[low]..], true),
    );
    if serialized_frame_len(&response)? > MAX_CONTROL_FRAME_LEN {
        anyhow::bail!("Service-control log response cannot fit within the frame limit");
    }
    Ok(response)
}

fn limit_remote_log_content(mut text: String, mut truncated: bool) -> (String, bool) {
    let initial_budget =
        MAX_REMOTE_SERVICE_LOG_BYTES - usize::from(truncated) * REMOTE_LOG_TRUNCATION_NOTICE.len();
    if text.len() > initial_budget {
        truncated = true;
        let content_budget = MAX_REMOTE_SERVICE_LOG_BYTES - REMOTE_LOG_TRUNCATION_NOTICE.len();
        let mut start = text.len() - content_budget;
        while !text.is_char_boundary(start) {
            start += 1;
        }
        text = text[start..].to_string();
    }
    (text, truncated)
}

fn render_remote_log_text(content: &str, truncated: bool) -> String {
    if truncated {
        format!("{REMOTE_LOG_TRUNCATION_NOTICE}{content}")
    } else {
        content.to_string()
    }
}

fn serialized_frame_len(value: &impl Serialize) -> Result<usize> {
    serde_json::to_vec(value)
        .map(|payload| payload.len())
        .map_err(|e| anyhow::anyhow!("Failed to serialize service-control frame: {e}"))
}

async fn write_frame<S, T>(stream: &mut S, value: &T) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
    T: Serialize,
{
    let payload = serde_json::to_vec(value)
        .map_err(|e| anyhow::anyhow!("Failed to serialize service-control frame: {e}"))?;
    if payload.len() > MAX_CONTROL_FRAME_LEN {
        anyhow::bail!(
            "Service-control frame too large: {} bytes (max {})",
            payload.len(),
            MAX_CONTROL_FRAME_LEN
        );
    }
    let payload_len = u32::try_from(payload.len())
        .map_err(|_| anyhow::anyhow!("Service-control frame is too large"))?;

    stream
        .write_all(&payload_len.to_be_bytes())
        .await
        .map_err(|e| anyhow::anyhow!("Failed to write frame length: {e}"))?;
    stream
        .write_all(&payload)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to write frame payload: {e}"))?;
    stream
        .flush()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to flush frame payload: {e}"))?;
    Ok(())
}

async fn read_frame<S, T>(stream: &mut S) -> Result<T>
where
    S: AsyncRead + AsyncWrite + Unpin,
    T: DeserializeOwned,
{
    let mut len_buf = [0u8; 4];
    stream
        .read_exact(&mut len_buf)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to read frame length: {e}"))?;
    let payload_len = u32::from_be_bytes(len_buf) as usize;
    if payload_len > MAX_CONTROL_FRAME_LEN {
        anyhow::bail!(
            "Service-control frame too large: {} bytes (max {})",
            payload_len,
            MAX_CONTROL_FRAME_LEN
        );
    }

    let mut payload = vec![0u8; payload_len];
    stream
        .read_exact(&mut payload)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to read frame payload: {e}"))?;
    serde_json::from_slice(&payload)
        .map_err(|e| anyhow::anyhow!("Failed to decode service-control frame: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_log_text_is_limited_from_the_front_at_a_character_boundary() {
        let logs = crate::runtime::BoundedLogText {
            text: format!("discarded\n{}", "界".repeat(MAX_REMOTE_SERVICE_LOG_BYTES)),
            truncated: false,
        };

        let response = bounded_service_log_response(None, "demo".to_string(), logs).unwrap();
        let limited = response.logs_text.unwrap();

        assert!(limited.starts_with(REMOTE_LOG_TRUNCATION_NOTICE));
        assert!(limited.ends_with('界'));
        assert!(limited.len() <= MAX_REMOTE_SERVICE_LOG_BYTES);
    }

    #[test]
    fn short_remote_log_text_is_unchanged() {
        let response = bounded_service_log_response(
            None,
            "demo".to_string(),
            crate::runtime::BoundedLogText {
                text: "hello\n".to_string(),
                truncated: false,
            },
        )
        .unwrap();
        assert_eq!(response.logs_text.as_deref(), Some("hello\n"));
    }

    #[test]
    fn control_heavy_remote_logs_fit_the_serialized_frame_limit() {
        let response = bounded_service_log_response(
            Some("request".to_string()),
            "demo".to_string(),
            crate::runtime::BoundedLogText {
                text: "\0".repeat(MAX_REMOTE_SERVICE_LOG_BYTES),
                truncated: false,
            },
        )
        .unwrap();
        let payload = serde_json::to_vec(&response).unwrap();

        assert!(payload.len() <= MAX_CONTROL_FRAME_LEN);
        assert!(
            response
                .logs_text
                .as_deref()
                .unwrap()
                .starts_with(REMOTE_LOG_TRUNCATION_NOTICE)
        );
    }

    #[tokio::test]
    async fn write_frame_rejects_oversized_outgoing_payloads() {
        let mut stream = futures::io::Cursor::new(Vec::new());
        let oversized = "\0".repeat(MAX_CONTROL_FRAME_LEN);

        let error = write_frame(&mut stream, &oversized).await.unwrap_err();

        assert!(error.to_string().contains("frame too large"));
        assert!(stream.into_inner().is_empty());
    }
}
