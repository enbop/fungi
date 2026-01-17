mod generated;

pub mod fungi_daemon_grpc {
    pub use crate::generated::*;
}

use std::net::SocketAddr;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use fungi_daemon_grpc::fungi_daemon_server::FungiDaemon;
use fungi_daemon_grpc::*;
use libp2p_identity::PeerId;
pub use tonic::{Request, Response, Status};

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
    inner: fungi_daemon::FungiDaemon,
}

impl FungiDaemonRpcImpl {
    pub fn new(inner: fungi_daemon::FungiDaemon) -> Self {
        Self { inner }
    }
}

#[tonic::async_trait]
impl FungiDaemon for FungiDaemonRpcImpl {
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
