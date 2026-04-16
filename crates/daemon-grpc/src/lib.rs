mod generated;

pub mod fungi_daemon_grpc {
    pub use crate::generated::*;
}

use std::collections::HashSet;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::pin::Pin;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use fungi_config::RelayAddressSource;
use fungi_daemon_grpc::fungi_daemon_server::FungiDaemon;
use fungi_daemon_grpc::*;
use libp2p_identity::PeerId;
use multiaddr::Multiaddr;
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tokio_stream::wrappers::ReceiverStream;
pub use tonic::{Request, Response, Status};

type PingEventSendError = mpsc::error::SendError<Result<PingPeerEvent, Status>>;

fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn system_time_to_unix_ms(time: SystemTime) -> i64 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn system_time_to_unix_ms_optional(time: Option<SystemTime>) -> i64 {
    time.map(system_time_to_unix_ms).unwrap_or(0)
}

fn ping_event(
    peer_id: &str,
    tick_seq: u64,
    ts_unix_ms: i64,
    event: ping_peer_event::Event,
) -> PingPeerEvent {
    PingPeerEvent {
        peer_id: peer_id.to_string(),
        tick_seq,
        ts_unix_ms,
        event: Some(event),
    }
}

fn proto_runtime_kind(kind: i32) -> Result<Option<fungi_daemon::RuntimeKind>, Status> {
    match ServiceRuntimeKind::try_from(kind) {
        Ok(ServiceRuntimeKind::Unspecified) => Ok(None),
        Ok(ServiceRuntimeKind::Docker) => Ok(Some(fungi_daemon::RuntimeKind::Docker)),
        Ok(ServiceRuntimeKind::Wasmtime) => Ok(Some(fungi_daemon::RuntimeKind::Wasmtime)),
        _ => Err(Status::invalid_argument("Invalid runtime kind")),
    }
}

impl PingPeerError {
    fn new(
        connection_id: impl Into<String>,
        direction: impl Into<String>,
        remote_addr: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            connection_id: connection_id.into(),
            direction: direction.into(),
            remote_addr: remote_addr.into(),
            message: message.into(),
        }
    }

    fn reason(message: impl Into<String>) -> Self {
        Self::new("", "", "", message)
    }

    fn event(self, peer_id: &str, tick_seq: u64, ts_unix_ms: i64) -> PingPeerEvent {
        ping_event(
            peer_id,
            tick_seq,
            ts_unix_ms,
            ping_peer_event::Event::Error(self),
        )
    }
}

pub async fn start_grpc_server(
    daemon: fungi_daemon::FungiDaemon,
    addr: SocketAddr,
) -> anyhow::Result<()> {
    tonic::transport::Server::builder()
        .add_service(
            fungi_daemon_grpc::fungi_daemon_server::FungiDaemonServer::new(
                FungiDaemonRpcImpl::new(daemon),
            ),
        )
        .serve(addr)
        .await?;
    Ok(())
}

pub struct FungiDaemonRpcImpl {
    inner: Arc<fungi_daemon::FungiDaemon>,
}

impl FungiDaemonRpcImpl {
    pub fn new(inner: fungi_daemon::FungiDaemon) -> Self {
        Self {
            inner: Arc::new(inner),
        }
    }
}

#[tonic::async_trait]
impl FungiDaemon for FungiDaemonRpcImpl {
    type PingPeerStream =
        Pin<Box<dyn tokio_stream::Stream<Item = Result<PingPeerEvent, Status>> + Send>>;

    async fn version(&self, _request: Request<Empty>) -> Result<Response<VersionResponse>, Status> {
        Ok(Response::new(VersionResponse {
            version: env!("CARGO_PKG_VERSION").to_string(),
        }))
    }

    async fn peer_id(&self, _request: Request<Empty>) -> Result<Response<PeerIdResponse>, Status> {
        let response = PeerIdResponse {
            peer_id: self.inner.peer_id(),
        };
        Ok(Response::new(response))
    }

    async fn config_file_path(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<ConfigFilePathResponse>, Status> {
        let response = ConfigFilePathResponse {
            config_file_path: self.inner.config_file_path(),
        };
        Ok(Response::new(response))
    }

    async fn hostname(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<HostnameResponse>, Status> {
        let response = HostnameResponse {
            hostname: self.inner.host_name().unwrap_or_default(),
        };
        Ok(Response::new(response))
    }

    async fn get_incoming_allowed_peers(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<IncomingAllowedPeersListResponse>, Status> {
        let response = IncomingAllowedPeersListResponse {
            peers: self
                .inner
                .get_incoming_allowed_peers()
                .into_iter()
                .map(peer_info_to_proto)
                .collect(),
        };
        Ok(Response::new(response))
    }

    async fn add_incoming_allowed_peer(
        &self,
        request: Request<AddIncomingAllowedPeerRequest>,
    ) -> Result<Response<Empty>, Status> {
        let peer_id = PeerId::from_str(&request.into_inner().peer_id)
            .map_err(|e| Status::invalid_argument(format!("Invalid peer_id: {}", e)))?;

        self.inner
            .add_incoming_allowed_peer(peer_id)
            .map_err(|e| Status::internal(format!("Failed to add peer: {}", e)))?;

        Ok(Response::new(Empty {}))
    }

    async fn remove_incoming_allowed_peer(
        &self,
        request: Request<RemoveIncomingAllowedPeerRequest>,
    ) -> Result<Response<Empty>, Status> {
        let peer_id = PeerId::from_str(&request.into_inner().peer_id)
            .map_err(|e| Status::invalid_argument(format!("Invalid peer_id: {}", e)))?;

        self.inner
            .remove_incoming_allowed_peer(peer_id)
            .map_err(|e| Status::internal(format!("Failed to remove peer: {}", e)))?;

        Ok(Response::new(Empty {}))
    }

    async fn get_relay_config(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<RelayConfigResponse>, Status> {
        let response = RelayConfigResponse {
            relay_enabled: self.inner.relay_enabled(),
            use_community_relays: self.inner.use_community_relays(),
            custom_relay_addresses: self
                .inner
                .custom_relay_addresses()
                .into_iter()
                .map(|address| address.to_string())
                .collect(),
            effective_relay_addresses: self
                .inner
                .effective_relay_addresses()
                .into_iter()
                .map(|entry| EffectiveRelayAddress {
                    address: entry.address.to_string(),
                    source: match entry.source {
                        RelayAddressSource::Community => "community".to_string(),
                        RelayAddressSource::Custom => "custom".to_string(),
                    },
                })
                .collect(),
        };
        Ok(Response::new(response))
    }

    async fn set_relay_enabled(
        &self,
        request: Request<RelayEnabledRequest>,
    ) -> Result<Response<Empty>, Status> {
        self.inner
            .set_relay_enabled(request.into_inner().enabled)
            .map_err(|e| {
                Status::internal(format!("Failed to update relay enabled state: {}", e))
            })?;
        Ok(Response::new(Empty {}))
    }

    async fn set_use_community_relays(
        &self,
        request: Request<UseCommunityRelaysRequest>,
    ) -> Result<Response<Empty>, Status> {
        self.inner
            .set_use_community_relays(request.into_inner().enabled)
            .map_err(|e| {
                Status::internal(format!("Failed to update community relay setting: {}", e))
            })?;
        Ok(Response::new(Empty {}))
    }

    async fn add_custom_relay_address(
        &self,
        request: Request<RelayAddressRequest>,
    ) -> Result<Response<Empty>, Status> {
        let address = request
            .into_inner()
            .address
            .parse::<Multiaddr>()
            .map_err(|e| Status::invalid_argument(format!("Invalid relay address: {}", e)))?;
        self.inner
            .add_custom_relay_address(address)
            .map_err(|e| Status::internal(format!("Failed to add custom relay address: {}", e)))?;
        Ok(Response::new(Empty {}))
    }

    async fn remove_custom_relay_address(
        &self,
        request: Request<RelayAddressRequest>,
    ) -> Result<Response<Empty>, Status> {
        let address = request
            .into_inner()
            .address
            .parse::<Multiaddr>()
            .map_err(|e| Status::invalid_argument(format!("Invalid relay address: {}", e)))?;
        self.inner
            .remove_custom_relay_address(address)
            .map_err(|e| {
                Status::internal(format!("Failed to remove custom relay address: {}", e))
            })?;
        Ok(Response::new(Empty {}))
    }

    async fn get_runtime_config(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<RuntimeConfigResponse>, Status> {
        let config = self.inner.get_runtime_config();
        Ok(Response::new(RuntimeConfigResponse {
            disable_docker: config.disable_docker,
            disable_wasmtime: config.disable_wasmtime,
            allowed_host_paths: config
                .allowed_host_paths
                .into_iter()
                .map(|path| path.to_string_lossy().to_string())
                .collect(),
            allowed_ports: config.allowed_ports.into_iter().map(i32::from).collect(),
            allowed_port_ranges: config
                .allowed_port_ranges
                .into_iter()
                .map(|range| RuntimeAllowedPortRange {
                    start: i32::from(range.start),
                    end: i32::from(range.end),
                })
                .collect(),
        }))
    }

    async fn get_local_runtime_status(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<LocalRuntimeStatusResponse>, Status> {
        let status = self.inner.local_runtime_status();
        Ok(Response::new(LocalRuntimeStatusResponse {
            docker: Some(RuntimeAvailabilityStatus {
                config_enabled: status.docker.config_enabled,
                detected: status.docker.detected,
                active: status.docker.active,
                endpoint: status.docker.endpoint.unwrap_or_default(),
            }),
            wasmtime: Some(RuntimeAvailabilityStatus {
                config_enabled: status.wasmtime.config_enabled,
                detected: status.wasmtime.detected,
                active: status.wasmtime.active,
                endpoint: status.wasmtime.endpoint.unwrap_or_default(),
            }),
        }))
    }

    async fn add_runtime_allowed_host_path(
        &self,
        request: Request<RuntimeAllowedHostPathRequest>,
    ) -> Result<Response<Empty>, Status> {
        let path = PathBuf::from(request.into_inner().path);
        self.inner
            .add_runtime_allowed_host_path(path)
            .map_err(|e| {
                Status::invalid_argument(format!("Failed to add runtime allowed host path: {}", e))
            })?;
        Ok(Response::new(Empty {}))
    }

    async fn remove_runtime_allowed_host_path(
        &self,
        request: Request<RuntimeAllowedHostPathRequest>,
    ) -> Result<Response<Empty>, Status> {
        let path = PathBuf::from(request.into_inner().path);
        self.inner
            .remove_runtime_allowed_host_path(&path)
            .map_err(|e| {
                Status::internal(format!("Failed to remove runtime allowed host path: {}", e))
            })?;
        Ok(Response::new(Empty {}))
    }

    async fn add_runtime_allowed_port(
        &self,
        request: Request<RuntimeAllowedPortRequest>,
    ) -> Result<Response<Empty>, Status> {
        let port = u16::try_from(request.into_inner().port)
            .map_err(|_| Status::invalid_argument("Invalid port"))?;
        self.inner
            .add_runtime_allowed_port(port)
            .map_err(|e| Status::internal(format!("Failed to add runtime allowed port: {}", e)))?;
        Ok(Response::new(Empty {}))
    }

    async fn remove_runtime_allowed_port(
        &self,
        request: Request<RuntimeAllowedPortRequest>,
    ) -> Result<Response<Empty>, Status> {
        let port = u16::try_from(request.into_inner().port)
            .map_err(|_| Status::invalid_argument("Invalid port"))?;
        self.inner.remove_runtime_allowed_port(port).map_err(|e| {
            Status::internal(format!("Failed to remove runtime allowed port: {}", e))
        })?;
        Ok(Response::new(Empty {}))
    }

    async fn add_runtime_allowed_port_range(
        &self,
        request: Request<RuntimeAllowedPortRangeRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let start = u16::try_from(req.start)
            .map_err(|_| Status::invalid_argument("Invalid range start"))?;
        let end =
            u16::try_from(req.end).map_err(|_| Status::invalid_argument("Invalid range end"))?;
        self.inner
            .add_runtime_allowed_port_range(start, end)
            .map_err(|e| {
                Status::internal(format!("Failed to add runtime allowed port range: {}", e))
            })?;
        Ok(Response::new(Empty {}))
    }

    async fn remove_runtime_allowed_port_range(
        &self,
        request: Request<RuntimeAllowedPortRangeRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let start = u16::try_from(req.start)
            .map_err(|_| Status::invalid_argument("Invalid range start"))?;
        let end =
            u16::try_from(req.end).map_err(|_| Status::invalid_argument("Invalid range end"))?;
        self.inner
            .remove_runtime_allowed_port_range(start, end)
            .map_err(|e| {
                Status::internal(format!(
                    "Failed to remove runtime allowed port range: {}",
                    e
                ))
            })?;
        Ok(Response::new(Empty {}))
    }

    async fn get_file_transfer_service_enabled(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<FileTransferServiceEnabledResponse>, Status> {
        let response = FileTransferServiceEnabledResponse {
            enabled: self.inner.get_file_transfer_service_enabled(),
        };
        Ok(Response::new(response))
    }

    async fn get_file_transfer_service_root_dir(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<FileTransferServiceRootDirResponse>, Status> {
        let response = FileTransferServiceRootDirResponse {
            root_dir: self
                .inner
                .get_file_transfer_service_root_dir()
                .to_string_lossy()
                .to_string(),
        };
        Ok(Response::new(response))
    }

    async fn start_file_transfer_service(
        &self,
        request: Request<StartFileTransferServiceRequest>,
    ) -> Result<Response<Empty>, Status> {
        let root_dir = request.into_inner().root_dir;

        self.inner
            .start_file_transfer_service(root_dir)
            .await
            .map_err(|e| Status::internal(format!("Failed to start service: {}", e)))?;

        Ok(Response::new(Empty {}))
    }

    async fn stop_file_transfer_service(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<Empty>, Status> {
        self.inner
            .stop_file_transfer_service()
            .map_err(|e| Status::internal(format!("Failed to stop service: {}", e)))?;

        Ok(Response::new(Empty {}))
    }

    async fn add_file_transfer_client(
        &self,
        request: Request<AddFileTransferClientRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let peer_id = PeerId::from_str(&req.peer_id)
            .map_err(|e| Status::invalid_argument(format!("Invalid peer_id: {}", e)))?;

        self.inner
            .add_file_transfer_client(
                req.enabled,
                if req.name.is_empty() {
                    None
                } else {
                    Some(req.name)
                },
                peer_id,
            )
            .await
            .map_err(|e| Status::internal(format!("Failed to add client: {}", e)))?;

        Ok(Response::new(Empty {}))
    }

    async fn remove_file_transfer_client(
        &self,
        request: Request<RemoveFileTransferClientRequest>,
    ) -> Result<Response<Empty>, Status> {
        let peer_id = PeerId::from_str(&request.into_inner().peer_id)
            .map_err(|e| Status::invalid_argument(format!("Invalid peer_id: {}", e)))?;

        self.inner
            .remove_file_transfer_client(peer_id)
            .map_err(|e| Status::internal(format!("Failed to remove client: {}", e)))?;

        Ok(Response::new(Empty {}))
    }

    async fn enable_file_transfer_client(
        &self,
        request: Request<EnableFileTransferClientRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let peer_id = PeerId::from_str(&req.peer_id)
            .map_err(|e| Status::invalid_argument(format!("Invalid peer_id: {}", e)))?;

        self.inner
            .enable_file_transfer_client(peer_id, req.enabled)
            .await
            .map_err(|e| Status::internal(format!("Failed to enable client: {}", e)))?;

        Ok(Response::new(Empty {}))
    }

    async fn get_all_file_transfer_clients(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<FileTransferClientsResponse>, Status> {
        let clients = self
            .inner
            .get_all_file_transfer_clients()
            .into_iter()
            .map(|c| FileTransferClient {
                enabled: c.enabled,
                name: c.name.unwrap_or_default(),
                peer_id: c.peer_id.to_string(),
            })
            .collect();

        let response = FileTransferClientsResponse { clients };
        Ok(Response::new(response))
    }

    async fn get_ftp_proxy(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<FtpProxyResponse>, Status> {
        let proxy = self.inner.get_ftp_proxy();
        let response = FtpProxyResponse {
            enabled: proxy.enabled,
            host: proxy.host.to_string(),
            port: proxy.port as i32,
        };
        Ok(Response::new(response))
    }

    async fn update_ftp_proxy(
        &self,
        request: Request<UpdateFtpProxyRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let host = req
            .host
            .parse()
            .map_err(|e| Status::invalid_argument(format!("Invalid host: {}", e)))?;

        self.inner
            .update_ftp_proxy(req.enabled, host, req.port as u16)
            .map_err(|e| Status::internal(format!("Failed to update FTP proxy: {}", e)))?;

        Ok(Response::new(Empty {}))
    }

    async fn get_webdav_proxy(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<WebdavProxyResponse>, Status> {
        let proxy = self.inner.get_webdav_proxy();
        let response = WebdavProxyResponse {
            enabled: proxy.enabled,
            host: proxy.host.to_string(),
            port: proxy.port as i32,
        };
        Ok(Response::new(response))
    }

    async fn update_webdav_proxy(
        &self,
        request: Request<UpdateWebdavProxyRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let host = req
            .host
            .parse()
            .map_err(|e| Status::invalid_argument(format!("Invalid host: {}", e)))?;

        self.inner
            .update_webdav_proxy(req.enabled, host, req.port as u16)
            .map_err(|e| Status::internal(format!("Failed to update WebDAV proxy: {}", e)))?;

        Ok(Response::new(Empty {}))
    }

    async fn get_tcp_tunneling_config(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<TcpTunnelingConfigResponse>, Status> {
        let config = self.inner.get_tcp_tunneling_config();
        let forwarding_rules = self
            .inner
            .get_tcp_forwarding_rules()
            .into_iter()
            .map(|(_, rule)| ForwardingRule {
                local_host: rule.local_host,
                local_port: rule.local_port as i32,
                remote_peer_id: rule.remote_peer_id,
                remote_port: rule.remote_port as i32,
                remote_protocol: rule.remote_protocol.unwrap_or_default(),
                remote_service_id: rule.remote_service_id.unwrap_or_default(),
                remote_service_name: rule.remote_service_name.unwrap_or_default(),
                remote_service_port_name: rule.remote_service_port_name.unwrap_or_default(),
            })
            .collect();

        let listening_rules = self
            .inner
            .get_tcp_listening_rules()
            .into_iter()
            .map(|(_, rule)| ListeningRule {
                host: rule.host,
                port: rule.port as i32,
                allowed_peers: vec![], // TODO: add allowed_peers to config
                protocol: rule.protocol.unwrap_or_default(),
            })
            .collect();

        let response = TcpTunnelingConfigResponse {
            forwarding_enabled: config.forwarding.enabled,
            listening_enabled: config.listening.enabled,
            forwarding_rules,
            listening_rules,
        };
        Ok(Response::new(response))
    }
    async fn add_tcp_forwarding_rule(
        &self,
        request: Request<AddTcpForwardingRuleRequest>,
    ) -> Result<Response<TcpForwardingRuleResponse>, Status> {
        let req = request.into_inner();

        let rule_id = self
            .inner
            .add_tcp_forwarding_rule_with_details(
                req.local_host,
                req.local_port as u16,
                req.peer_id,
                req.remote_port as u16,
                empty_to_none(req.remote_protocol),
                empty_to_none(req.remote_service_id),
                empty_to_none(req.remote_service_name),
                empty_to_none(req.remote_service_port_name),
            )
            .await
            .map_err(|e| Status::internal(format!("Failed to add forwarding rule: {}", e)))?;

        let response = TcpForwardingRuleResponse { rule_id };
        Ok(Response::new(response))
    }

    async fn remove_tcp_forwarding_rule(
        &self,
        request: Request<RemoveTcpForwardingRuleRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        self.inner
            .remove_tcp_forwarding_rule_with_protocol(
                req.local_host,
                req.local_port as u16,
                req.peer_id,
                req.remote_port as u16,
                empty_to_none(req.remote_protocol),
            )
            .map_err(|e| Status::internal(format!("Failed to remove forwarding rule: {}", e)))?;

        Ok(Response::new(Empty {}))
    }

    async fn add_tcp_listening_rule(
        &self,
        request: Request<AddTcpListeningRuleRequest>,
    ) -> Result<Response<TcpListeningRuleResponse>, Status> {
        let req = request.into_inner();

        let rule_id = self
            .inner
            .add_tcp_listening_rule_with_protocol(
                req.local_host,
                req.local_port as u16,
                empty_to_none(req.protocol),
            )
            .await
            .map_err(|e| Status::internal(format!("Failed to add listening rule: {}", e)))?;

        let response = TcpListeningRuleResponse { rule_id };
        Ok(Response::new(response))
    }

    async fn remove_tcp_listening_rule(
        &self,
        request: Request<RemoveTcpListeningRuleRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        self.inner
            .remove_tcp_listening_rule_with_protocol(
                req.local_host,
                req.local_port as u16,
                empty_to_none(req.protocol),
            )
            .map_err(|e| Status::internal(format!("Failed to remove listening rule: {}", e)))?;

        Ok(Response::new(Empty {}))
    }

    async fn list_mdns_devices(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<PeerInfoListResponse>, Status> {
        let peers = self
            .inner
            .mdns_get_local_devices()
            .await
            .map_err(|e| Status::internal(format!("Failed to get local devices: {}", e)))?
            .into_iter()
            .map(peer_info_to_proto)
            .collect();

        let response = PeerInfoListResponse { peers };
        Ok(Response::new(response))
    }

    async fn list_address_book_peers(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<PeerInfoListResponse>, Status> {
        let peers = self
            .inner
            .address_book_get_all()
            .into_iter()
            .map(peer_info_to_proto)
            .collect();

        let response = PeerInfoListResponse { peers };
        Ok(Response::new(response))
    }

    async fn update_address_book_peer(
        &self,
        request: Request<UpdateAddressBookPeerRequest>,
    ) -> Result<Response<Empty>, Status> {
        let peer_info = request
            .into_inner()
            .peer_info
            .ok_or_else(|| Status::invalid_argument("peer_info is required"))?;

        let peer_info = proto_to_peer_info(peer_info)
            .map_err(|e| Status::invalid_argument(format!("Invalid peer_info: {}", e)))?;

        self.inner
            .address_book_add_or_update(peer_info)
            .map_err(|e| Status::internal(format!("Failed to add/update peer: {}", e)))?;

        Ok(Response::new(Empty {}))
    }

    async fn get_address_book_peer(
        &self,
        request: Request<GetAddressBookPeerRequest>,
    ) -> Result<Response<PeerInfoResponse>, Status> {
        let peer_id = PeerId::from_str(&request.into_inner().peer_id)
            .map_err(|e| Status::invalid_argument(format!("Invalid peer_id: {}", e)))?;

        let peer_info = self
            .inner
            .address_book_get_peer(peer_id)
            .map(peer_info_to_proto);

        let response = PeerInfoResponse { peer_info };
        Ok(Response::new(response))
    }

    async fn remove_address_book_peer(
        &self,
        request: Request<RemoveAddressBookPeerRequest>,
    ) -> Result<Response<Empty>, Status> {
        let peer_id = PeerId::from_str(&request.into_inner().peer_id)
            .map_err(|e| Status::invalid_argument(format!("Invalid peer_id: {}", e)))?;

        self.inner
            .address_book_remove(peer_id)
            .map_err(|e| Status::internal(format!("Failed to remove peer: {}", e)))?;

        Ok(Response::new(Empty {}))
    }

    async fn ping_peer(
        &self,
        request: Request<PingPeerRequest>,
    ) -> Result<Response<Self::PingPeerStream>, Status> {
        let req = request.into_inner();
        let peer_id = PeerId::from_str(&req.peer_id)
            .map_err(|e| Status::invalid_argument(format!("Invalid peer_id: {}", e)))?;
        let interval_ms = if req.interval_ms == 0 {
            2000
        } else {
            req.interval_ms
        };

        let daemon = self.inner.clone();
        let (tx, rx) = mpsc::channel::<Result<PingPeerEvent, Status>>(128);
        let run = async move {
            let mut ticker = tokio::time::interval(Duration::from_millis(interval_ms as u64));
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            let per_ping_timeout_ms = (interval_ms.saturating_sub(100)).max(200);
            let per_ping_timeout = Duration::from_millis(per_ping_timeout_ms as u64);

            let peer_id_str = peer_id.to_string();
            let mut tick_seq = 0_u64;

            tx.send(Ok(ping_event(
                &peer_id_str,
                tick_seq,
                now_unix_ms(),
                ping_peer_event::Event::Connecting(PingPeerConnecting {}),
            )))
            .await?;

            if let Err(e) = daemon.dial_peer_once(peer_id).await {
                tx.send(Ok(PingPeerError::reason(e.to_string()).event(
                    &peer_id_str,
                    tick_seq,
                    now_unix_ms(),
                )))
                .await?;
            }

            loop {
                ticker.tick().await;
                tick_seq += 1;
                let ts_unix_ms = now_unix_ms();

                let Some(peer_connections) = daemon.get_peer_connections(peer_id) else {
                    tx.send(Ok(ping_event(
                        &peer_id_str,
                        tick_seq,
                        ts_unix_ms,
                        ping_peer_event::Event::Idle(PingPeerIdle {}),
                    )))
                    .await?;
                    continue;
                };

                if peer_connections.outbound().is_empty() {
                    tx.send(Ok(ping_event(
                        &peer_id_str,
                        tick_seq,
                        ts_unix_ms,
                        ping_peer_event::Event::Idle(PingPeerIdle {}),
                    )))
                    .await?;
                    continue;
                }

                let mut ping_set = JoinSet::new();
                let mut seen_addrs = HashSet::new();
                for conn in peer_connections.outbound().iter() {
                    let connection_id = conn.connection_id();
                    let remote_addr = conn.multiaddr().to_string();
                    if !seen_addrs.insert(remote_addr.clone()) {
                        continue;
                    }
                    let daemon = daemon.clone();
                    let peer_id = peer_id;
                    ping_set.spawn(async move {
                        let res = daemon
                            .ping_peer_connection(peer_id, connection_id, per_ping_timeout)
                            .await;
                        (connection_id, "outbound".to_string(), remote_addr, res)
                    });
                }

                while let Some(join_res) = ping_set.join_next().await {
                    match join_res {
                        Ok((connection_id, direction, remote_addr, Ok(rtt))) => {
                            tx.send(Ok(ping_event(
                                &peer_id_str,
                                tick_seq,
                                ts_unix_ms,
                                ping_peer_event::Event::Result(PingPeerResult {
                                    connection_id: connection_id.to_string(),
                                    direction,
                                    remote_addr,
                                    rtt_ms: rtt.as_millis() as u64,
                                }),
                            )))
                            .await?;
                        }
                        Ok((connection_id, direction, remote_addr, Err(e))) => {
                            tx.send(Ok(PingPeerError::new(
                                connection_id.to_string(),
                                direction,
                                remote_addr,
                                e.to_string(),
                            )
                            .event(&peer_id_str, tick_seq, ts_unix_ms)))
                                .await?;
                        }
                        Err(e) => {
                            tx.send(Ok(PingPeerError::reason(format!(
                                "Ping task join error: {e}"
                            ))
                            .event(&peer_id_str, tick_seq, ts_unix_ms)))
                                .await?;
                        }
                    }
                }
            }

            #[allow(unreachable_code)]
            Ok::<(), PingEventSendError>(())
        };

        tokio::spawn(run);

        Ok(Response::new(
            Box::pin(ReceiverStream::new(rx)) as Self::PingPeerStream
        ))
    }

    async fn list_connections(
        &self,
        request: Request<ListConnectionsRequest>,
    ) -> Result<Response<ListConnectionsResponse>, Status> {
        let peer_filter = request.into_inner().peer_id;
        let peer_filter = if peer_filter.trim().is_empty() {
            None
        } else {
            Some(
                PeerId::from_str(&peer_filter)
                    .map_err(|e| Status::invalid_argument(format!("Invalid peer_id: {}", e)))?,
            )
        };

        let connections = self
            .inner
            .list_connections(peer_filter)
            .into_iter()
            .map(|c| ConnectionSnapshot {
                peer_id: c.peer_id,
                connection_id: c.connection_id,
                direction: c.direction,
                remote_addr: c.remote_addr,
                is_relay: c.is_relay,
                last_rtt_ms: c.last_rtt_ms,
                last_ping_unix_ms: system_time_to_unix_ms_optional(c.last_ping_at),
                active_streams_total: c.active_streams_total as u64,
                active_streams_by_protocol: c
                    .active_streams_by_protocol
                    .into_iter()
                    .map(|s| ProtocolStreamCountSnapshot {
                        protocol_name: s.protocol_name,
                        stream_count: s.stream_count as u64,
                    })
                    .collect(),
                policy_state: c.policy_state,
                policy_reason: c.policy_reason,
            })
            .collect();

        Ok(Response::new(ListConnectionsResponse { connections }))
    }

    async fn list_active_streams(
        &self,
        request: Request<ListActiveStreamsRequest>,
    ) -> Result<Response<ListActiveStreamsResponse>, Status> {
        let req = request.into_inner();

        let peer_filter = if req.peer_id.trim().is_empty() {
            None
        } else {
            Some(
                PeerId::from_str(&req.peer_id)
                    .map_err(|e| Status::invalid_argument(format!("Invalid peer_id: {}", e)))?,
            )
        };
        let protocol_filter = if req.protocol_name.trim().is_empty() {
            None
        } else {
            Some(req.protocol_name)
        };

        let mut streams = if let Some(protocol_name) = protocol_filter {
            self.inner.list_active_streams_by_protocol(protocol_name)
        } else {
            self.inner.list_active_streams()
        };

        if let Some(peer_id) = peer_filter {
            let peer_id_string = peer_id.to_string();
            streams.retain(|s| s.peer_id == peer_id_string);
        }

        let streams = streams
            .into_iter()
            .map(|s| ActiveStreamSnapshot {
                stream_id: s.stream_id,
                peer_id: s.peer_id,
                connection_id: s.connection_id,
                protocol_name: s.protocol_name,
                opened_at_unix_ms: system_time_to_unix_ms(s.opened_at),
            })
            .collect();

        Ok(Response::new(ListActiveStreamsResponse { streams }))
    }

    async fn list_external_address_candidates(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<ListExternalAddressCandidatesResponse>, Status> {
        let candidates = self
            .inner
            .list_external_address_candidates()
            .into_iter()
            .map(|candidate| ExternalAddressSnapshot {
                address: candidate.address,
                transport: candidate.transport,
                freshness: candidate.freshness,
                recommend_refresh_before_dcutr: candidate.recommend_refresh_before_dcutr,
                first_observed_at_unix_ms: system_time_to_unix_ms(candidate.first_observed_at),
                last_observed_at_unix_ms: system_time_to_unix_ms(candidate.last_observed_at),
                confirmed_at_unix_ms: system_time_to_unix_ms_optional(candidate.confirmed_at),
                expired_at_unix_ms: system_time_to_unix_ms_optional(candidate.expired_at),
                observation_count: candidate.observation_count,
                sources: candidate.sources,
            })
            .collect();

        Ok(Response::new(ListExternalAddressCandidatesResponse {
            candidates,
        }))
    }

    async fn list_relay_endpoint_statuses(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<ListRelayEndpointStatusesResponse>, Status> {
        let statuses = self
            .inner
            .list_relay_endpoint_statuses()
            .into_iter()
            .map(|status| RelayEndpointStatusSnapshot {
                relay_addr: status.relay_addr,
                relay_peer_id: status.relay_peer_id.unwrap_or_default(),
                transport: status.transport,
                listener_registered: status.listener_registered,
                task_running: status.task_running,
                current_direct_connection_id: status
                    .current_direct_connection_id
                    .unwrap_or_default(),
                last_listener_seen_at_unix_ms: system_time_to_unix_ms_optional(
                    status.last_listener_seen_at,
                ),
                last_listener_missing_at_unix_ms: system_time_to_unix_ms_optional(
                    status.last_listener_missing_at,
                ),
                last_reservation_accepted_at_unix_ms: system_time_to_unix_ms_optional(
                    status.last_reservation_accepted_at,
                ),
                last_reservation_established_at_unix_ms: system_time_to_unix_ms_optional(
                    status.last_reservation_established_at,
                ),
                last_reservation_renewed_at_unix_ms: system_time_to_unix_ms_optional(
                    status.last_reservation_renewed_at,
                ),
                last_direct_connection_closed_at_unix_ms: system_time_to_unix_ms_optional(
                    status.last_direct_connection_closed_at,
                ),
                last_management_action: status.last_management_action.unwrap_or_default(),
                last_error: status.last_error.unwrap_or_default(),
            })
            .collect();

        Ok(Response::new(ListRelayEndpointStatusesResponse {
            statuses,
        }))
    }

    async fn list_peer_addresses(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<ListPeerAddressesResponse>, Status> {
        let addresses = self
            .inner
            .list_peer_addresses()
            .into_iter()
            .map(|address| PeerAddressSnapshot {
                peer_id: address.peer_id,
                address: address.address,
                transport: address.transport,
                source: address.source,
                first_observed_at_unix_ms: system_time_to_unix_ms(address.first_observed_at),
                last_observed_at_unix_ms: system_time_to_unix_ms(address.last_observed_at),
                observation_count: address.observation_count,
            })
            .collect();

        Ok(Response::new(ListPeerAddressesResponse { addresses }))
    }

    async fn pull_service(
        &self,
        request: Request<PullServiceRequest>,
    ) -> Result<Response<ServiceInstanceResponse>, Status> {
        let req = request.into_inner();
        let instance = self
            .inner
            .pull_service_from_manifest_yaml(
                req.manifest_yaml,
                if req.manifest_base_dir.trim().is_empty() {
                    None
                } else {
                    Some(std::path::PathBuf::from(req.manifest_base_dir))
                },
            )
            .await
            .map_err(|e| Status::internal(format!("Failed to pull service: {e}")))?;

        let instance_json = serde_json::to_string(&instance)
            .map_err(|e| Status::internal(format!("Failed to serialize service instance: {e}")))?;
        Ok(Response::new(ServiceInstanceResponse { instance_json }))
    }

    async fn start_service(
        &self,
        request: Request<ServiceNameRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let runtime = proto_runtime_kind(req.runtime)?;
        match runtime {
            Some(runtime) => self
                .inner
                .start_service(runtime, req.name)
                .await
                .map_err(|e| Status::internal(format!("Failed to start service: {e}")))?,
            None => self
                .inner
                .start_service_by_name(req.name)
                .await
                .map_err(|e| Status::internal(format!("Failed to start service: {e}")))?,
        }
        Ok(Response::new(Empty {}))
    }

    async fn stop_service(
        &self,
        request: Request<ServiceNameRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let runtime = proto_runtime_kind(req.runtime)?;
        match runtime {
            Some(runtime) => self
                .inner
                .stop_service(runtime, req.name)
                .await
                .map_err(|e| Status::internal(format!("Failed to stop service: {e}")))?,
            None => self
                .inner
                .stop_service_by_name(req.name)
                .await
                .map_err(|e| Status::internal(format!("Failed to stop service: {e}")))?,
        }
        Ok(Response::new(Empty {}))
    }

    async fn remove_service(
        &self,
        request: Request<ServiceNameRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let runtime = proto_runtime_kind(req.runtime)?;
        match runtime {
            Some(runtime) => self
                .inner
                .remove_service(runtime, req.name)
                .await
                .map_err(|e| Status::internal(format!("Failed to remove service: {e}")))?,
            None => self
                .inner
                .remove_service_by_name(req.name)
                .await
                .map_err(|e| Status::internal(format!("Failed to remove service: {e}")))?,
        }
        Ok(Response::new(Empty {}))
    }

    async fn inspect_service(
        &self,
        request: Request<ServiceNameRequest>,
    ) -> Result<Response<ServiceInstanceResponse>, Status> {
        let req = request.into_inner();
        let runtime = proto_runtime_kind(req.runtime)?;
        let instance = match runtime {
            Some(runtime) => self
                .inner
                .inspect_service(runtime, req.name)
                .await
                .map_err(|e| Status::internal(format!("Failed to inspect service: {e}")))?,
            None => self
                .inner
                .inspect_service_by_name(req.name)
                .await
                .map_err(|e| Status::internal(format!("Failed to inspect service: {e}")))?,
        };
        let instance_json = serde_json::to_string(&instance)
            .map_err(|e| Status::internal(format!("Failed to serialize service instance: {e}")))?;
        Ok(Response::new(ServiceInstanceResponse { instance_json }))
    }

    async fn get_service_logs(
        &self,
        request: Request<GetServiceLogsRequest>,
    ) -> Result<Response<ServiceLogsResponse>, Status> {
        let req = request.into_inner();
        let runtime = proto_runtime_kind(req.runtime)?;
        let tail = if req.tail.trim().is_empty() {
            None
        } else {
            Some(req.tail)
        };
        let logs = match runtime {
            Some(runtime) => self
                .inner
                .get_service_logs(runtime, req.name, tail)
                .await
                .map_err(|e| Status::internal(format!("Failed to get service logs: {e}")))?,
            None => self
                .inner
                .get_service_logs_by_name(req.name, tail)
                .await
                .map_err(|e| Status::internal(format!("Failed to get service logs: {e}")))?,
        };

        Ok(Response::new(ServiceLogsResponse {
            raw: logs.raw,
            text: logs.text,
        }))
    }

    async fn list_services(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<ListServicesResponse>, Status> {
        let services = self
            .inner
            .list_services()
            .await
            .map_err(|e| Status::internal(format!("Failed to list services: {e}")))?;
        let services_json = serde_json::to_string(&services)
            .map_err(|e| Status::internal(format!("Failed to serialize services: {e}")))?;
        Ok(Response::new(ListServicesResponse { services_json }))
    }

    async fn list_peer_catalog(
        &self,
        request: Request<ListPeerCatalogRequest>,
    ) -> Result<Response<ListPeerCatalogResponse>, Status> {
        let req = request.into_inner();
        let peer_id = PeerId::from_str(&req.peer_id)
            .map_err(|e| Status::invalid_argument(format!("Invalid peer_id: {}", e)))?;
        let services = self
            .inner
            .list_peer_catalog(peer_id)
            .await
            .map_err(|e| Status::internal(format!("Failed to list peer catalog: {e}")))?;
        let services_json = serde_json::to_string(&services)
            .map_err(|e| Status::internal(format!("Failed to serialize peer catalog: {e}")))?;
        Ok(Response::new(ListPeerCatalogResponse { services_json }))
    }

    async fn get_peer_capability_summary(
        &self,
        request: Request<GetPeerCapabilitySummaryRequest>,
    ) -> Result<Response<GetPeerCapabilitySummaryResponse>, Status> {
        let req = request.into_inner();
        let peer_id = PeerId::from_str(&req.peer_id)
            .map_err(|e| Status::invalid_argument(format!("Invalid peer_id: {}", e)))?;
        let capability_summary = self
            .inner
            .get_peer_capability_summary(peer_id)
            .await
            .map_err(|e| Status::internal(format!("Failed to get peer capability summary: {e}")))?;
        let capability_summary_json = serde_json::to_string(&capability_summary).map_err(|e| {
            Status::internal(format!("Failed to serialize peer capability summary: {e}"))
        })?;
        Ok(Response::new(GetPeerCapabilitySummaryResponse {
            capability_summary_json,
        }))
    }

    async fn remote_pull_service(
        &self,
        request: Request<RemotePullServiceRequest>,
    ) -> Result<Response<RemoteServiceControlResponse>, Status> {
        let req = request.into_inner();
        let peer_id = PeerId::from_str(&req.peer_id)
            .map_err(|e| Status::invalid_argument(format!("Invalid peer_id: {}", e)))?;

        let response = self
            .inner
            .remote_pull_service(peer_id, req.manifest_yaml)
            .await
            .map_err(|e| Status::internal(format!("Failed to pull remote service: {e}")))?;

        Ok(Response::new(RemoteServiceControlResponse {
            service_name: response
                .service
                .map(|service| service.name)
                .unwrap_or_default(),
        }))
    }

    async fn remote_start_service(
        &self,
        request: Request<RemoteServiceNameRequest>,
    ) -> Result<Response<RemoteServiceControlResponse>, Status> {
        let req = request.into_inner();
        let peer_id = PeerId::from_str(&req.peer_id)
            .map_err(|e| Status::invalid_argument(format!("Invalid peer_id: {}", e)))?;

        let response = self
            .inner
            .remote_start_service(peer_id, req.name)
            .await
            .map_err(|e| Status::internal(format!("Failed to start remote service: {e}")))?;

        Ok(Response::new(RemoteServiceControlResponse {
            service_name: response
                .service
                .map(|service| service.name)
                .unwrap_or_default(),
        }))
    }

    async fn remote_stop_service(
        &self,
        request: Request<RemoteServiceNameRequest>,
    ) -> Result<Response<RemoteServiceControlResponse>, Status> {
        let req = request.into_inner();
        let peer_id = PeerId::from_str(&req.peer_id)
            .map_err(|e| Status::invalid_argument(format!("Invalid peer_id: {}", e)))?;

        let response = self
            .inner
            .remote_stop_service(peer_id, req.name)
            .await
            .map_err(|e| Status::internal(format!("Failed to stop remote service: {e}")))?;

        Ok(Response::new(RemoteServiceControlResponse {
            service_name: response
                .service
                .map(|service| service.name)
                .unwrap_or_default(),
        }))
    }

    async fn remote_remove_service(
        &self,
        request: Request<RemoteServiceNameRequest>,
    ) -> Result<Response<RemoteServiceControlResponse>, Status> {
        let req = request.into_inner();
        let peer_id = PeerId::from_str(&req.peer_id)
            .map_err(|e| Status::invalid_argument(format!("Invalid peer_id: {}", e)))?;

        let response = self
            .inner
            .remote_remove_service(peer_id, req.name)
            .await
            .map_err(|e| Status::internal(format!("Failed to remove remote service: {e}")))?;

        Ok(Response::new(RemoteServiceControlResponse {
            service_name: response
                .service
                .map(|service| service.name)
                .unwrap_or_default(),
        }))
    }

    async fn remote_list_services(
        &self,
        request: Request<RemotePeerRequest>,
    ) -> Result<Response<ListServicesResponse>, Status> {
        let req = request.into_inner();
        let peer_id = PeerId::from_str(&req.peer_id)
            .map_err(|e| Status::invalid_argument(format!("Invalid peer_id: {}", e)))?;

        let response = self
            .inner
            .remote_list_services(peer_id)
            .await
            .map_err(|e| Status::internal(format!("Failed to list remote services: {e}")))?;

        Ok(Response::new(ListServicesResponse {
            services_json: response.services_json.unwrap_or_default(),
        }))
    }

    async fn attach_service_access(
        &self,
        request: Request<AttachServiceAccessRequest>,
    ) -> Result<Response<ServiceAccessResponse>, Status> {
        let req = request.into_inner();
        let peer_id = PeerId::from_str(&req.peer_id)
            .map_err(|e| Status::invalid_argument(format!("Invalid peer_id: {}", e)))?;

        let service_access = self
            .inner
            .attach_service_access(peer_id, req.service_id)
            .await
            .map_err(|e| Status::internal(format!("Failed to attach service access: {e}")))?;

        let service_access_json = serde_json::to_string(&service_access)
            .map_err(|e| Status::internal(format!("Failed to serialize service access: {e}")))?;

        Ok(Response::new(ServiceAccessResponse {
            service_access_json,
        }))
    }

    async fn detach_service_access(
        &self,
        request: Request<DetachServiceAccessRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let peer_id = PeerId::from_str(&req.peer_id)
            .map_err(|e| Status::invalid_argument(format!("Invalid peer_id: {}", e)))?;

        self.inner
            .detach_service_access(peer_id, req.service_id)
            .map_err(|e| Status::internal(format!("Failed to detach service access: {e}")))?;

        Ok(Response::new(Empty {}))
    }

    async fn list_service_accesses(
        &self,
        request: Request<ListServiceAccessesRequest>,
    ) -> Result<Response<ServiceAccessesResponse>, Status> {
        let req = request.into_inner();
        let peer_id = if req.peer_id.trim().is_empty() {
            None
        } else {
            Some(
                PeerId::from_str(&req.peer_id)
                    .map_err(|e| Status::invalid_argument(format!("Invalid peer_id: {}", e)))?,
            )
        };

        let service_accesses = self.inner.list_service_accesses(peer_id);
        let service_accesses_json = serde_json::to_string(&service_accesses)
            .map_err(|e| Status::internal(format!("Failed to serialize service accesses: {e}")))?;

        Ok(Response::new(ServiceAccessesResponse {
            service_accesses_json,
        }))
    }
}

fn empty_to_none(value: String) -> Option<String> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value)
    }
}

// Helper functions to convert between domain and proto types
fn peer_info_to_proto(info: fungi_config::address_book::PeerInfo) -> PeerInfo {
    PeerInfo {
        peer_id: info.peer_id.to_string(),
        alias: info.alias.unwrap_or_default(),
        hostname: info.hostname.unwrap_or_default(),
        os: os_to_string(info.os),
        public_ip: info.public_ip.unwrap_or_default(),
        private_ips: info.private_ips,
        created_at: system_time_to_i64(info.created_at),
        last_connected: system_time_to_i64(info.last_connected),
        version: info.version,
    }
}

fn proto_to_peer_info(proto: PeerInfo) -> Result<fungi_config::address_book::PeerInfo, String> {
    let peer_id =
        PeerId::from_str(&proto.peer_id).map_err(|e| format!("Invalid peer_id: {}", e))?;

    Ok(fungi_config::address_book::PeerInfo {
        peer_id,
        alias: if proto.alias.is_empty() {
            None
        } else {
            Some(proto.alias)
        },
        hostname: if proto.hostname.is_empty() {
            None
        } else {
            Some(proto.hostname)
        },
        multiaddrs: vec![],
        os: string_to_os(&proto.os),
        public_ip: if proto.public_ip.is_empty() {
            None
        } else {
            Some(proto.public_ip)
        },
        private_ips: proto.private_ips,
        created_at: i64_to_system_time(proto.created_at),
        last_connected: i64_to_system_time(proto.last_connected),
        version: proto.version,
    })
}

fn os_to_string(os: fungi_config::address_book::Os) -> String {
    match os {
        fungi_config::address_book::Os::Windows => "Windows".to_string(),
        fungi_config::address_book::Os::MacOS => "MacOS".to_string(),
        fungi_config::address_book::Os::Linux => "Linux".to_string(),
        fungi_config::address_book::Os::Android => "Android".to_string(),
        fungi_config::address_book::Os::IOS => "IOS".to_string(),
        fungi_config::address_book::Os::Unknown => "Unknown".to_string(),
    }
}

fn string_to_os(s: &str) -> fungi_config::address_book::Os {
    match s {
        "Windows" => fungi_config::address_book::Os::Windows,
        "MacOS" => fungi_config::address_book::Os::MacOS,
        "Linux" => fungi_config::address_book::Os::Linux,
        "Android" => fungi_config::address_book::Os::Android,
        "IOS" => fungi_config::address_book::Os::IOS,
        _ => fungi_config::address_book::Os::Unknown,
    }
}

fn system_time_to_i64(time: SystemTime) -> i64 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn i64_to_system_time(secs: i64) -> SystemTime {
    UNIX_EPOCH + std::time::Duration::from_secs(secs.max(0) as u64)
}
