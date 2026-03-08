mod generated;

pub mod fungi_daemon_grpc {
    pub use crate::generated::*;
}

use std::collections::HashSet;
use std::net::SocketAddr;
use std::pin::Pin;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use fungi_daemon_grpc::fungi_daemon_server::FungiDaemon;
use fungi_daemon_grpc::*;
use libp2p_identity::PeerId;
use serde_json;
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
            .add_tcp_forwarding_rule(
                req.local_host,
                req.local_port as u16,
                req.peer_id,
                req.remote_port as u16,
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
            .remove_tcp_forwarding_rule(
                req.local_host,
                req.local_port as u16,
                req.peer_id,
                req.remote_port as u16,
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
            .add_tcp_listening_rule(req.local_host, req.local_port as u16, req.allowed_peers)
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
            .remove_tcp_listening_rule(req.local_host, req.local_port as u16)
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
                    let peer_id = peer_id.clone();
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

    async fn deploy_service(
        &self,
        request: Request<DeployServiceRequest>,
    ) -> Result<Response<ServiceInstanceResponse>, Status> {
        let req = request.into_inner();
        let instance = self
            .inner
            .deploy_service_from_manifest_yaml(
                req.manifest_yaml,
                if req.manifest_base_dir.trim().is_empty() {
                    None
                } else {
                    Some(std::path::PathBuf::from(req.manifest_base_dir))
                },
            )
            .await
            .map_err(|e| Status::internal(format!("Failed to deploy service: {e}")))?;

        let instance_json = serde_json::to_string(&instance)
            .map_err(|e| Status::internal(format!("Failed to serialize service instance: {e}")))?;
        Ok(Response::new(ServiceInstanceResponse { instance_json }))
    }

    async fn start_service(
        &self,
        request: Request<ServiceHandleRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let runtime = proto_runtime_kind(req.runtime)?;
        match runtime {
            Some(runtime) => self
                .inner
                .start_service(runtime, req.handle)
                .await
                .map_err(|e| Status::internal(format!("Failed to start service: {e}")))?,
            None => self
                .inner
                .start_service_by_handle(req.handle)
                .await
                .map_err(|e| Status::internal(format!("Failed to start service: {e}")))?,
        }
        Ok(Response::new(Empty {}))
    }

    async fn stop_service(
        &self,
        request: Request<ServiceHandleRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let runtime = proto_runtime_kind(req.runtime)?;
        match runtime {
            Some(runtime) => self
                .inner
                .stop_service(runtime, req.handle)
                .await
                .map_err(|e| Status::internal(format!("Failed to stop service: {e}")))?,
            None => self
                .inner
                .stop_service_by_handle(req.handle)
                .await
                .map_err(|e| Status::internal(format!("Failed to stop service: {e}")))?,
        }
        Ok(Response::new(Empty {}))
    }

    async fn remove_service(
        &self,
        request: Request<ServiceHandleRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let runtime = proto_runtime_kind(req.runtime)?;
        match runtime {
            Some(runtime) => self
                .inner
                .remove_service(runtime, req.handle)
                .await
                .map_err(|e| Status::internal(format!("Failed to remove service: {e}")))?,
            None => self
                .inner
                .remove_service_by_handle(req.handle)
                .await
                .map_err(|e| Status::internal(format!("Failed to remove service: {e}")))?,
        }
        Ok(Response::new(Empty {}))
    }

    async fn inspect_service(
        &self,
        request: Request<ServiceHandleRequest>,
    ) -> Result<Response<ServiceInstanceResponse>, Status> {
        let req = request.into_inner();
        let runtime = proto_runtime_kind(req.runtime)?;
        let instance = match runtime {
            Some(runtime) => self
                .inner
                .inspect_service(runtime, req.handle)
                .await
                .map_err(|e| Status::internal(format!("Failed to inspect service: {e}")))?,
            None => self
                .inner
                .inspect_service_by_handle(req.handle)
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
                .get_service_logs(runtime, req.handle, tail)
                .await
                .map_err(|e| Status::internal(format!("Failed to get service logs: {e}")))?,
            None => self
                .inner
                .get_service_logs_by_handle(req.handle, tail)
                .await
                .map_err(|e| Status::internal(format!("Failed to get service logs: {e}")))?,
        };

        Ok(Response::new(ServiceLogsResponse {
            raw: logs.raw,
            text: logs.text,
        }))
    }

    async fn discover_peer_services(
        &self,
        request: Request<DiscoverPeerServicesRequest>,
    ) -> Result<Response<DiscoverPeerServicesResponse>, Status> {
        let req = request.into_inner();
        let peer_id = PeerId::from_str(&req.peer_id)
            .map_err(|e| Status::invalid_argument(format!("Invalid peer_id: {}", e)))?;
        let services = self
            .inner
            .discover_peer_services(peer_id)
            .await
            .map_err(|e| Status::internal(format!("Failed to discover peer services: {e}")))?;
        let services_json = serde_json::to_string(&services)
            .map_err(|e| Status::internal(format!("Failed to serialize peer services: {e}")))?;
        Ok(Response::new(DiscoverPeerServicesResponse {
            services_json,
        }))
    }

    async fn discover_peer_capabilities(
        &self,
        request: Request<DiscoverPeerCapabilitiesRequest>,
    ) -> Result<Response<DiscoverPeerCapabilitiesResponse>, Status> {
        let req = request.into_inner();
        let peer_id = PeerId::from_str(&req.peer_id)
            .map_err(|e| Status::invalid_argument(format!("Invalid peer_id: {}", e)))?;
        let capabilities = self
            .inner
            .discover_peer_capabilities(peer_id)
            .await
            .map_err(|e| Status::internal(format!("Failed to discover peer capabilities: {e}")))?;
        let capabilities_json = serde_json::to_string(&capabilities)
            .map_err(|e| Status::internal(format!("Failed to serialize peer capabilities: {e}")))?;
        Ok(Response::new(DiscoverPeerCapabilitiesResponse {
            capabilities_json,
        }))
    }

    async fn remote_deploy_service(
        &self,
        request: Request<RemoteDeployServiceRequest>,
    ) -> Result<Response<RemoteServiceControlResponse>, Status> {
        let req = request.into_inner();
        let peer_id = PeerId::from_str(&req.peer_id)
            .map_err(|e| Status::invalid_argument(format!("Invalid peer_id: {}", e)))?;

        let response = self
            .inner
            .remote_deploy_service(peer_id, req.manifest_yaml)
            .await
            .map_err(|e| Status::internal(format!("Failed to deploy remote service: {e}")))?;

        Ok(Response::new(RemoteServiceControlResponse {
            service_name: response
                .service
                .map(|service| service.name)
                .unwrap_or_default(),
        }))
    }

    async fn remote_start_service(
        &self,
        request: Request<RemoteServiceHandleRequest>,
    ) -> Result<Response<RemoteServiceControlResponse>, Status> {
        let req = request.into_inner();
        let peer_id = PeerId::from_str(&req.peer_id)
            .map_err(|e| Status::invalid_argument(format!("Invalid peer_id: {}", e)))?;

        let response = self
            .inner
            .remote_start_service(peer_id, req.handle)
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
        request: Request<RemoteServiceHandleRequest>,
    ) -> Result<Response<RemoteServiceControlResponse>, Status> {
        let req = request.into_inner();
        let peer_id = PeerId::from_str(&req.peer_id)
            .map_err(|e| Status::invalid_argument(format!("Invalid peer_id: {}", e)))?;

        let response = self
            .inner
            .remote_stop_service(peer_id, req.handle)
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
        request: Request<RemoteServiceHandleRequest>,
    ) -> Result<Response<RemoteServiceControlResponse>, Status> {
        let req = request.into_inner();
        let peer_id = PeerId::from_str(&req.peer_id)
            .map_err(|e| Status::invalid_argument(format!("Invalid peer_id: {}", e)))?;

        let response = self
            .inner
            .remote_remove_service(peer_id, req.handle)
            .await
            .map_err(|e| Status::internal(format!("Failed to remove remote service: {e}")))?;

        Ok(Response::new(RemoteServiceControlResponse {
            service_name: response
                .service
                .map(|service| service.name)
                .unwrap_or_default(),
        }))
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
