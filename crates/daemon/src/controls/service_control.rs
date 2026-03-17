use std::{collections::HashSet, path::PathBuf, sync::Arc, time::Duration};

use anyhow::Result;
use fungi_config::FungiConfig;
use fungi_swarm::{ConnectionSelectionStrategy, SwarmControl};
use fungi_util::protocols::FUNGI_SERVICE_CONTROL_PROTOCOL;
use futures::StreamExt;
use futures::{AsyncRead, AsyncWrite};
use libp2p::{
    PeerId,
    futures::{AsyncReadExt, AsyncWriteExt},
};
use libp2p_stream::IncomingStreams;
use parking_lot::{Mutex, RwLock};
use serde::{Serialize, de::DeserializeOwned};

use crate::controls::TcpTunnelingControl;
use crate::{
    ManifestResolutionPolicy, RuntimeControl, ServiceControlRequest, ServiceControlResponse,
    ServiceManifest, service_expose_endpoint_bindings,
};

const MAX_CONTROL_FRAME_LEN: usize = 2 * 1024 * 1024;

#[derive(Clone)]
pub struct ServiceControlProtocolControl {
    swarm_control: SwarmControl,
    config: Arc<Mutex<FungiConfig>>,
    fungi_home: PathBuf,
    runtime_control: RuntimeControl,
    tcp_tunneling_control: TcpTunnelingControl,
    incoming_allowed_peers: Arc<RwLock<HashSet<PeerId>>>,
}

impl ServiceControlProtocolControl {
    const CONNECT_SNIFF_WAIT: Duration = Duration::from_secs(3);

    pub fn new(
        swarm_control: SwarmControl,
        config: Arc<Mutex<FungiConfig>>,
        fungi_home: PathBuf,
        runtime_control: RuntimeControl,
        tcp_tunneling_control: TcpTunnelingControl,
        incoming_allowed_peers: Arc<RwLock<HashSet<PeerId>>>,
    ) -> Self {
        Self {
            swarm_control,
            config,
            fungi_home,
            runtime_control,
            tcp_tunneling_control,
            incoming_allowed_peers,
        }
    }

    pub fn start(&self) -> Result<()> {
        let incoming_streams = self
            .swarm_control
            .accept_incoming_streams(FUNGI_SERVICE_CONTROL_PROTOCOL)
            .map_err(anyhow::Error::from)?;
        let this = self.clone();
        tokio::spawn(async move {
            this.listen_from_libp2p_stream(incoming_streams).await;
        });
        Ok(())
    }

    pub async fn pull_peer_service(
        &self,
        peer_id: PeerId,
        manifest_yaml: String,
    ) -> Result<ServiceControlResponse> {
        self.send_request(
            peer_id,
            ServiceControlRequest::PullService {
                request_id: None,
                manifest_yaml,
            },
        )
        .await
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
        let (mut stream, _handle, _connection_id) = self
            .swarm_control
            .open_stream_with_strategy(
                peer_id,
                FUNGI_SERVICE_CONTROL_PROTOCOL,
                ConnectionSelectionStrategy::PreferDirect,
                Self::CONNECT_SNIFF_WAIT,
            )
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
            })?
            .into_result()
    }

    async fn listen_from_libp2p_stream(self, mut incoming_streams: IncomingStreams) {
        while let Some((peer_id, mut stream)) = incoming_streams.next().await {
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

                let response = if !this.incoming_allowed_peers.read().contains(&peer_id) {
                    log::warn!("Deny service control from disallowed peer: {peer_id}");
                    ServiceControlResponse::error(
                        request.request_id().map(str::to_string),
                        "permission_denied",
                        format!("peer {peer_id} is not allowed to control this node"),
                    )
                } else {
                    this.handle_request(request).await
                };

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
                self.runtime_control
                    .pull_manifest_yaml(&manifest_yaml, &self.fungi_home, &self.fungi_home, &policy)
                    .await
                    .map(|instance| instance.name)
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
        let config = self.config.lock();
        ManifestResolutionPolicy {
            allowed_tcp_ports: config.runtime.allowed_ports.clone(),
            allowed_tcp_port_ranges: config.runtime.allowed_port_ranges.clone(),
        }
    }

    async fn sync_service_endpoint_listeners_by_name(
        &self,
        name: &str,
        enabled: bool,
    ) -> Result<()> {
        let manifest = self.runtime_control.get_service_manifest(name);
        self.sync_service_endpoint_listeners_for_manifest(manifest.as_ref(), enabled)
            .await
    }

    async fn sync_service_endpoint_listeners_for_manifest(
        &self,
        manifest: Option<&ServiceManifest>,
        enabled: bool,
    ) -> Result<()> {
        let Some(manifest) = manifest else {
            return Ok(());
        };

        let endpoints = service_expose_endpoint_bindings(manifest);
        let listening_rules = self.tcp_tunneling_control.get_listening_rules();

        for endpoint in endpoints {
            let existing_rule_id = listening_rules
                .iter()
                .find(|(_, rule)| {
                    rule.port == endpoint.host_port
                        && rule.protocol.as_deref() == Some(endpoint.protocol.as_str())
                })
                .map(|(rule_id, _)| rule_id.clone());

            if enabled {
                if existing_rule_id.is_none() {
                    self.tcp_tunneling_control
                        .add_listening_rule(fungi_config::tcp_tunneling::ListeningRule {
                            host: "127.0.0.1".to_string(),
                            port: endpoint.host_port,
                            protocol: Some(endpoint.protocol),
                        })
                        .await?;
                }
            } else if let Some(rule_id) = existing_rule_id {
                self.tcp_tunneling_control.remove_listening_rule(&rule_id)?;
            }
        }

        Ok(())
    }
}

async fn write_frame<S, T>(stream: &mut S, value: &T) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
    T: Serialize,
{
    let payload = serde_json::to_vec(value)
        .map_err(|e| anyhow::anyhow!("Failed to serialize service-control frame: {e}"))?;
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
