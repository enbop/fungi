use std::collections::{BTreeMap, BTreeSet};
use std::net::TcpListener as StdTcpListener;
use std::time::Duration;
use std::{net::IpAddr, path::PathBuf};

use anyhow::{Result, bail};
use fungi_config::address_book::PeerInfo;
use fungi_config::file_transfer::{FileTransferClient, FtpProxy, WebdavProxy};
use fungi_swarm::PeerConnections;
use libp2p::PeerId;
use libp2p::swarm::ConnectionId;
use serde::{Deserialize, Serialize};

use crate::runtime::{
    DiscoveredService, RuntimeKind, ServiceInstance, ServiceLogs, ServiceLogsOptions,
    ServiceManifest, service_expose_endpoint_bindings,
};
use crate::{
    FungiDaemon, ManifestResolutionPolicy, NodeCapabilities, ServiceControlResponse,
    build_local_node_capabilities,
};

#[derive(Debug, Clone)]
pub struct ConnectionSnapshot {
    pub peer_id: String,
    pub connection_id: String,
    pub direction: String,
    pub remote_addr: String,
    pub is_relay: bool,
    pub last_rtt_ms: u64,
    pub last_ping_at: Option<std::time::SystemTime>,
    pub active_streams_total: usize,
    pub active_streams_by_protocol: Vec<ProtocolStreamCountSnapshot>,
}

#[derive(Debug, Clone)]
pub struct ProtocolStreamCountSnapshot {
    pub protocol_name: String,
    pub stream_count: usize,
}

#[derive(Debug, Clone)]
pub struct ActiveStreamSnapshot {
    pub stream_id: u64,
    pub peer_id: String,
    pub connection_id: String,
    pub protocol_name: String,
    pub opened_at: std::time::SystemTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnabledRemoteService {
    pub peer_id: String,
    pub service_id: String,
    pub service_name: String,
    pub endpoints: Vec<EnabledRemoteServiceEndpoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnabledRemoteServiceEndpoint {
    pub name: String,
    pub protocol: String,
    pub local_host: String,
    pub local_port: u16,
}

impl FungiDaemon {
    pub fn host_name(&self) -> Option<String> {
        self.config().lock().get_hostname()
    }

    #[cfg(target_os = "android")]
    pub fn init_mobile_device_name(name: String) {
        {
            fungi_util::init_mobile_device_name(name);
        }
    }

    pub fn peer_id(&self) -> String {
        self.swarm_control().local_peer_id().to_string()
    }

    pub fn config_file_path(&self) -> String {
        self.config()
            .lock()
            .config_file_path()
            .to_string_lossy()
            .to_string()
    }

    pub fn add_incoming_allowed_peer(&self, peer_id: PeerId) -> Result<()> {
        // update config and write config file
        let current_config = self.config().lock().clone();
        let updated_config = current_config.add_incoming_allowed_peer(&peer_id)?;
        *self.config().lock() = updated_config;

        // update state
        self.swarm_control()
            .state()
            .incoming_allowed_peers()
            .write()
            .insert(peer_id);
        Ok(())
    }

    pub fn remove_incoming_allowed_peer(&self, peer_id: PeerId) -> Result<()> {
        // update config and write config file
        let current_config = self.config().lock().clone();
        let updated_config = current_config.remove_incoming_allowed_peer(&peer_id)?;
        *self.config().lock() = updated_config;
        // update state
        self.swarm_control()
            .state()
            .incoming_allowed_peers()
            .write()
            .remove(&peer_id);
        // TODO disconnect connected incoming peer
        Ok(())
    }

    pub fn get_file_transfer_service_enabled(&self) -> bool {
        self.config().lock().file_transfer.server.enabled
    }

    pub fn get_file_transfer_service_root_dir(&self) -> PathBuf {
        self.config()
            .lock()
            .file_transfer
            .server
            .shared_root_dir
            .clone()
    }

    pub fn get_peer_connections(&self, peer_id: PeerId) -> Option<PeerConnections> {
        self.swarm_control().state().get_peer_connections(&peer_id)
    }

    pub fn list_connections(&self, peer_id: Option<PeerId>) -> Vec<ConnectionSnapshot> {
        let state = self.swarm_control().state();
        let peer_connections = state.peer_connections();
        let peer_connections = peer_connections.lock();

        let mut snapshots = Vec::new();
        for (pid, peer_conn) in peer_connections.iter() {
            if let Some(filter_peer_id) = peer_id
                && *pid != filter_peer_id
            {
                continue;
            }

            for conn in peer_conn.inbound() {
                let ping_info = state.connection_ping_info(&conn.connection_id());
                let (last_rtt_ms, last_ping_at) = match ping_info {
                    Some(info) => match (info.last_rtt, info.last_rtt_at) {
                        (Some(last_rtt), Some(last_rtt_at)) => {
                            (last_rtt.as_millis() as u64, Some(last_rtt_at))
                        }
                        _ => (0, None),
                    },
                    None => (0, None),
                };

                let active_streams_by_protocol = state
                    .connection_active_stream_protocol_counts(&conn.connection_id())
                    .into_iter()
                    .map(
                        |(protocol_name, stream_count)| ProtocolStreamCountSnapshot {
                            protocol_name,
                            stream_count,
                        },
                    )
                    .collect::<Vec<_>>();
                let active_streams_total = active_streams_by_protocol
                    .iter()
                    .map(|entry| entry.stream_count)
                    .sum();

                let remote_addr = conn.multiaddr().to_string();
                snapshots.push(ConnectionSnapshot {
                    peer_id: pid.to_string(),
                    connection_id: conn.connection_id().to_string(),
                    direction: "inbound".to_string(),
                    is_relay: remote_addr.contains("/p2p-circuit"),
                    remote_addr,
                    last_rtt_ms,
                    last_ping_at,
                    active_streams_total,
                    active_streams_by_protocol,
                });
            }

            for conn in peer_conn.outbound() {
                let ping_info = state.connection_ping_info(&conn.connection_id());
                let (last_rtt_ms, last_ping_at) = match ping_info {
                    Some(info) => match (info.last_rtt, info.last_rtt_at) {
                        (Some(last_rtt), Some(last_rtt_at)) => {
                            (last_rtt.as_millis() as u64, Some(last_rtt_at))
                        }
                        _ => (0, None),
                    },
                    _ => (0, None),
                };

                let active_streams_by_protocol = state
                    .connection_active_stream_protocol_counts(&conn.connection_id())
                    .into_iter()
                    .map(
                        |(protocol_name, stream_count)| ProtocolStreamCountSnapshot {
                            protocol_name,
                            stream_count,
                        },
                    )
                    .collect::<Vec<_>>();
                let active_streams_total = active_streams_by_protocol
                    .iter()
                    .map(|entry| entry.stream_count)
                    .sum();

                let remote_addr = conn.multiaddr().to_string();
                snapshots.push(ConnectionSnapshot {
                    peer_id: pid.to_string(),
                    connection_id: conn.connection_id().to_string(),
                    direction: "outbound".to_string(),
                    is_relay: remote_addr.contains("/p2p-circuit"),
                    remote_addr,
                    last_rtt_ms,
                    last_ping_at,
                    active_streams_total,
                    active_streams_by_protocol,
                });
            }
        }

        snapshots.sort_by(|a, b| {
            a.peer_id
                .cmp(&b.peer_id)
                .then(a.direction.cmp(&b.direction))
                .then(a.connection_id.cmp(&b.connection_id))
        });

        snapshots
    }

    pub fn list_active_streams(&self) -> Vec<ActiveStreamSnapshot> {
        let mut streams = self
            .swarm_control()
            .state()
            .list_active_streams()
            .into_iter()
            .map(|stream| ActiveStreamSnapshot {
                stream_id: stream.stream_id,
                peer_id: stream.peer_id.to_string(),
                connection_id: stream.connection_id.to_string(),
                protocol_name: stream.protocol_name,
                opened_at: stream.opened_at,
            })
            .collect::<Vec<_>>();

        streams.sort_by(|a, b| a.stream_id.cmp(&b.stream_id));
        streams
    }

    pub fn list_active_streams_by_protocol(
        &self,
        protocol_name: String,
    ) -> Vec<ActiveStreamSnapshot> {
        let mut streams = self
            .swarm_control()
            .state()
            .active_streams_by_protocol(&protocol_name)
            .into_iter()
            .map(|stream| ActiveStreamSnapshot {
                stream_id: stream.stream_id,
                peer_id: stream.peer_id.to_string(),
                connection_id: stream.connection_id.to_string(),
                protocol_name: stream.protocol_name,
                opened_at: stream.opened_at,
            })
            .collect::<Vec<_>>();

        streams.sort_by(|a, b| a.stream_id.cmp(&b.stream_id));
        streams
    }

    pub async fn dial_peer_once(&self, peer_id: PeerId) -> Result<()> {
        self.swarm_control()
            .connect(peer_id)
            .await
            .map_err(|e| anyhow::anyhow!("Dial failed: {e}"))?;
        Ok(())
    }

    pub async fn ping_peer_connection(
        &self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        timeout: Duration,
    ) -> Result<std::time::Duration> {
        self.swarm_control()
            .ping_connection(peer_id, connection_id, timeout)
            .await
    }

    pub async fn start_file_transfer_service(&self, root_dir: String) -> Result<()> {
        if self.get_file_transfer_service_root_dir().to_str() == Some(&root_dir)
            && self
                .fts_control()
                .has_service(&PathBuf::from(root_dir.clone()))
        {
            bail!(
                "File transfer service is already running with the root directory: {}",
                root_dir
            );
        }

        // update config and write config file
        let current_config = self.config().lock().clone();
        let (updated_config, service_config) =
            current_config.update_file_transfer_service(true, PathBuf::from(root_dir.clone()))?;
        *self.config().lock() = updated_config;

        self.fts_control().stop_all();
        // FIXME: This is a workaround to ensure the service is stopped before starting it again.
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        self.fts_control()
            .clone()
            .add_service(service_config)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to start file transfer service: {}", e))?;
        Ok(())
    }

    pub fn stop_file_transfer_service(&self) -> Result<()> {
        let current_root_dir = self.get_file_transfer_service_root_dir();
        if !self.fts_control().has_service(&current_root_dir) {
            bail!("File transfer service is not running.");
        }

        // update config and write config file
        let current_config = self.config().lock().clone();
        let (updated_config, _) =
            current_config.update_file_transfer_service(false, current_root_dir)?;
        *self.config().lock() = updated_config;

        self.fts_control().stop_all();
        Ok(())
    }

    pub async fn add_file_transfer_client(
        &self,
        enabled: bool,
        mut name: Option<String>,
        peer_id: PeerId,
    ) -> Result<()> {
        let ftc_control = self.ftc_control();

        // TODO add transaction
        if name.is_none()
            && let Ok(remote_host_name) = ftc_control.connect_and_get_host_name(peer_id).await
        {
            name = remote_host_name
        }

        let current_config = self.config().lock().clone();
        let updated_config =
            current_config.add_file_transfer_client(enabled, peer_id, name.clone())?;
        *self.config().lock() = updated_config;

        if enabled {
            ftc_control.add_client(FileTransferClient {
                enabled,
                peer_id,
                name,
            });
        }

        Ok(())
    }

    pub fn remove_file_transfer_client(&self, peer_id: PeerId) -> Result<()> {
        let current_config = self.config().lock().clone();
        let updated_config = current_config.remove_file_transfer_client(&peer_id)?;
        *self.config().lock() = updated_config;
        self.ftc_control().remove_client(&peer_id);
        Ok(())
    }

    pub async fn enable_file_transfer_client(&self, peer_id: PeerId, enabled: bool) -> Result<()> {
        let current_config = self.config().lock().clone();
        let (updated_config, client_option) =
            current_config.enable_file_transfer_client(&peer_id, enabled)?;
        *self.config().lock() = updated_config;

        let Some(mut config) = client_option else {
            bail!("File transfer client with peer_id {} not found", peer_id);
        };
        if enabled {
            if self.ftc_control().has_client(&peer_id) {
                return Ok(());
            }

            // TODO add transaction
            if config.name.is_none()
                && let Ok(remote_host_name) =
                    self.ftc_control().connect_and_get_host_name(peer_id).await
            {
                config.name = remote_host_name
            }
            self.ftc_control().add_client(config);
        } else {
            self.ftc_control().remove_client(&peer_id);
        }

        Ok(())
    }

    pub fn get_all_file_transfer_clients(&self) -> Vec<FileTransferClient> {
        self.config().lock().file_transfer.client.clone()
    }

    pub fn get_ftp_proxy(&self) -> FtpProxy {
        self.config().lock().file_transfer.proxy_ftp.clone()
    }

    pub fn update_ftp_proxy(&self, enabled: bool, host: IpAddr, port: u16) -> Result<()> {
        if port == 0 {
            bail!("Port must be greater than 0");
        }
        let current_config = self.config().lock().clone();
        let updated_config = current_config.update_ftp_proxy(enabled, host, port)?;
        *self.config().lock() = updated_config;
        self.update_ftp_proxy_task(enabled, host, port)
    }

    pub fn get_webdav_proxy(&self) -> WebdavProxy {
        self.config().lock().file_transfer.proxy_webdav.clone()
    }

    pub fn update_webdav_proxy(&self, enabled: bool, host: IpAddr, port: u16) -> Result<()> {
        if port == 0 {
            bail!("Port must be greater than 0");
        }
        let current_config = self.config().lock().clone();
        let updated_config = current_config.update_webdav_proxy(enabled, host, port)?;
        *self.config().lock() = updated_config;
        self.update_webdav_proxy_task(enabled, host, port)
    }

    // TCP Tunneling API methods
    pub fn get_tcp_forwarding_rules(
        &self,
    ) -> Vec<(String, fungi_config::tcp_tunneling::ForwardingRule)> {
        self.tcp_tunneling_control().get_forwarding_rules()
    }

    pub fn get_tcp_listening_rules(
        &self,
    ) -> Vec<(String, fungi_config::tcp_tunneling::ListeningRule)> {
        self.tcp_tunneling_control().get_listening_rules()
    }

    pub async fn add_tcp_forwarding_rule(
        &self,
        local_host: String,
        local_port: u16,
        remote_peer_id: String,
        remote_port: u16,
    ) -> Result<String> {
        self.add_tcp_forwarding_rule_with_details(
            local_host,
            local_port,
            remote_peer_id,
            remote_port,
            None,
            None,
            None,
            None,
        )
        .await
    }

    pub async fn add_tcp_forwarding_rule_with_details(
        &self,
        local_host: String,
        local_port: u16,
        remote_peer_id: String,
        remote_port: u16,
        remote_protocol: Option<String>,
        remote_service_id: Option<String>,
        remote_service_name: Option<String>,
        remote_service_port_name: Option<String>,
    ) -> Result<String> {
        let rule = fungi_config::tcp_tunneling::ForwardingRule {
            local_host,
            local_port,
            remote_peer_id,
            remote_protocol,
            remote_port,
            remote_service_id,
            remote_service_name,
            remote_service_port_name,
        };
        self.add_tcp_forwarding_rule_internal(rule).await
    }

    pub fn remove_tcp_forwarding_rule(
        &self,
        local_host: String,
        local_port: u16,
        remote_peer_id: String,
        remote_port: u16,
    ) -> Result<()> {
        self.remove_tcp_forwarding_rule_with_protocol(
            local_host,
            local_port,
            remote_peer_id,
            remote_port,
            None,
        )
    }

    pub fn remove_tcp_forwarding_rule_with_protocol(
        &self,
        local_host: String,
        local_port: u16,
        remote_peer_id: String,
        remote_port: u16,
        remote_protocol: Option<String>,
    ) -> Result<()> {
        let rules = self.tcp_tunneling_control().get_forwarding_rules();
        let rule_id = rules
            .iter()
            .find(|(_, rule)| {
                rule.local_host == local_host
                    && rule.local_port == local_port
                    && rule.remote_peer_id == remote_peer_id
                    && rule.remote_protocol == remote_protocol
                    && rule.remote_port == remote_port
            })
            .map(|(id, _)| id.clone())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Forwarding rule not found: {}:{} -> {}:{}",
                    local_host,
                    local_port,
                    remote_peer_id,
                    remote_port
                )
            })?;

        self.remove_tcp_forwarding_rule_internal(&rule_id)
    }

    pub async fn add_tcp_listening_rule(
        &self,
        local_host: String,
        local_port: u16,
        _allowed_peers: Vec<String>,
    ) -> Result<String> {
        self.add_tcp_listening_rule_with_protocol(local_host, local_port, None)
            .await
    }

    pub async fn add_tcp_listening_rule_with_protocol(
        &self,
        local_host: String,
        local_port: u16,
        protocol: Option<String>,
    ) -> Result<String> {
        let rule = fungi_config::tcp_tunneling::ListeningRule {
            host: local_host,
            port: local_port,
            protocol,
        };
        self.add_tcp_listening_rule_internal(rule).await
    }

    pub async fn enable_remote_service(
        &self,
        peer_id: PeerId,
        service_id: String,
    ) -> Result<EnabledRemoteService> {
        let discovered_services = self.discover_peer_services(peer_id).await?;
        let service = discovered_services
            .into_iter()
            .find(|service| service.service_id == service_id)
            .ok_or_else(|| anyhow::anyhow!("remote service not found: {}", service_id))?;

        if service.endpoints.is_empty() {
            bail!(
                "remote service exposes no named TCP endpoints: {}",
                service.service_id
            );
        }

        let peer_id_string = peer_id.to_string();
        let existing_rules = self.get_tcp_forwarding_rules();
        let mut reserved_local_ports = existing_rules
            .iter()
            .map(|(_, rule)| rule.local_port)
            .collect::<BTreeSet<_>>();
        let mut enabled_endpoints = Vec::new();

        for endpoint in service.endpoints {
            if let Some((_, rule)) = existing_rules.iter().find(|(_, rule)| {
                rule.remote_peer_id == peer_id_string
                    && rule.remote_service_id.as_deref() == Some(service.service_id.as_str())
                    && rule.remote_service_port_name.as_deref() == Some(endpoint.name.as_str())
            }) {
                enabled_endpoints.push(EnabledRemoteServiceEndpoint {
                    name: endpoint.name,
                    protocol: endpoint.protocol,
                    local_host: rule.local_host.clone(),
                    local_port: rule.local_port,
                });
                continue;
            }

            let local_port =
                allocate_local_forward_port(endpoint.service_port, &reserved_local_ports)?;
            reserved_local_ports.insert(local_port);

            self.tcp_tunneling_control()
                .add_forwarding_rule(fungi_config::tcp_tunneling::ForwardingRule {
                    local_host: "127.0.0.1".to_string(),
                    local_port,
                    remote_peer_id: peer_id_string.clone(),
                    remote_protocol: Some(endpoint.protocol.clone()),
                    remote_port: 0,
                    remote_service_id: Some(service.service_id.clone()),
                    remote_service_name: Some(service.service_name.clone()),
                    remote_service_port_name: Some(endpoint.name.clone()),
                })
                .await?;

            enabled_endpoints.push(EnabledRemoteServiceEndpoint {
                name: endpoint.name,
                protocol: endpoint.protocol,
                local_host: "127.0.0.1".to_string(),
                local_port,
            });
        }

        enabled_endpoints.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(EnabledRemoteService {
            peer_id: peer_id_string,
            service_id: service.service_id,
            service_name: service.service_name,
            endpoints: enabled_endpoints,
        })
    }

    pub fn disable_remote_service(&self, peer_id: PeerId, service_id: String) -> Result<()> {
        self.disable_remote_service_by_match(peer_id, &service_id)
    }

    pub fn disable_remote_service_by_match(&self, peer_id: PeerId, matcher: &str) -> Result<()> {
        let peer_id_string = peer_id.to_string();
        let rules_to_remove = self
            .get_tcp_forwarding_rules()
            .into_iter()
            .filter(|(_, rule)| {
                rule.remote_peer_id == peer_id_string
                    && (rule.remote_service_id.as_deref() == Some(matcher)
                        || rule.remote_service_name.as_deref() == Some(matcher))
            })
            .map(|(rule_id, _)| rule_id)
            .collect::<Vec<_>>();

        for rule_id in rules_to_remove {
            self.remove_tcp_forwarding_rule_internal(&rule_id)?;
        }

        Ok(())
    }

    pub fn list_enabled_remote_services(
        &self,
        peer_id: Option<PeerId>,
    ) -> Vec<EnabledRemoteService> {
        let peer_filter = peer_id.map(|peer_id| peer_id.to_string());
        let mut grouped =
            BTreeMap::<(String, String, String), Vec<EnabledRemoteServiceEndpoint>>::new();

        for (_, rule) in self.get_tcp_forwarding_rules() {
            let Some(service_id) = rule.remote_service_id.clone() else {
                continue;
            };
            let Some(service_name) = rule.remote_service_name.clone() else {
                continue;
            };
            let Some(endpoint_name) = rule.remote_service_port_name.clone() else {
                continue;
            };
            if let Some(peer_filter) = &peer_filter
                && &rule.remote_peer_id != peer_filter
            {
                continue;
            }

            grouped
                .entry((rule.remote_peer_id.clone(), service_id, service_name))
                .or_default()
                .push(EnabledRemoteServiceEndpoint {
                    name: endpoint_name,
                    protocol: rule.remote_protocol.clone().unwrap_or_default(),
                    local_host: rule.local_host.clone(),
                    local_port: rule.local_port,
                });
        }

        let mut services = grouped
            .into_iter()
            .map(|((peer_id, service_id, service_name), mut endpoints)| {
                endpoints.sort_by(|left, right| left.name.cmp(&right.name));
                EnabledRemoteService {
                    peer_id,
                    service_id,
                    service_name,
                    endpoints,
                }
            })
            .collect::<Vec<_>>();
        services.sort_by(|left, right| {
            left.peer_id
                .cmp(&right.peer_id)
                .then(left.service_id.cmp(&right.service_id))
        });
        services
    }

    pub fn remove_tcp_listening_rule(&self, local_host: String, local_port: u16) -> Result<()> {
        self.remove_tcp_listening_rule_with_protocol(local_host, local_port, None)
    }

    pub fn remove_tcp_listening_rule_with_protocol(
        &self,
        local_host: String,
        local_port: u16,
        protocol: Option<String>,
    ) -> Result<()> {
        let rules = self.tcp_tunneling_control().get_listening_rules();
        let rule_id = rules
            .iter()
            .find(|(_, rule)| {
                rule.host == local_host && rule.port == local_port && rule.protocol == protocol
            })
            .map(|(id, _)| id.clone())
            .ok_or_else(|| {
                anyhow::anyhow!("Listening rule not found: {}:{}", local_host, local_port)
            })?;

        self.remove_tcp_listening_rule_internal(&rule_id)
    }

    pub fn get_tcp_tunneling_config(&self) -> fungi_config::tcp_tunneling::TcpTunneling {
        self.config().lock().tcp_tunneling.clone()
    }

    pub fn docker_enabled(&self) -> bool {
        self.config().lock().docker.enabled
    }

    async fn sync_service_endpoint_listeners_by_handle(
        &self,
        handle: &str,
        enabled: bool,
    ) -> Result<()> {
        let manifest = self.runtime_control().get_service_manifest(handle);
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
        let listening_rules = self.get_tcp_listening_rules();

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
                    self.tcp_tunneling_control()
                        .add_listening_rule(fungi_config::tcp_tunneling::ListeningRule {
                            host: "127.0.0.1".to_string(),
                            port: endpoint.host_port,
                            protocol: Some(endpoint.protocol),
                        })
                        .await?;
                }
            } else if let Some(rule_id) = existing_rule_id {
                self.tcp_tunneling_control()
                    .remove_listening_rule(&rule_id)?;
            }
        }

        Ok(())
    }

    pub fn get_docker_config(&self) -> fungi_config::docker::Docker {
        self.config().lock().docker.clone()
    }

    pub fn supports_runtime(&self, runtime: RuntimeKind) -> bool {
        self.runtime_control().supports(runtime)
    }

    fn fungi_home_dir(&self) -> PathBuf {
        self.config()
            .lock()
            .config_file_path()
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .to_path_buf()
    }

    fn manifest_resolution_policy(&self) -> ManifestResolutionPolicy {
        let config_handle = self.config();
        let config = config_handle.lock();
        ManifestResolutionPolicy {
            allowed_tcp_ports: config.docker.allowed_ports.clone(),
            allowed_tcp_port_ranges: config.docker.allowed_port_ranges.clone(),
        }
    }

    pub async fn deploy_service(&self, manifest: ServiceManifest) -> Result<ServiceInstance> {
        self.runtime_control().deploy(&manifest).await
    }

    pub async fn deploy_service_from_manifest_yaml(
        &self,
        manifest_yaml: String,
        manifest_base_dir: Option<PathBuf>,
    ) -> Result<ServiceInstance> {
        let fungi_home = self.fungi_home_dir();
        let base_dir = manifest_base_dir.unwrap_or_else(|| fungi_home.clone());
        let policy = self.manifest_resolution_policy();
        self.runtime_control()
            .deploy_manifest_yaml(&manifest_yaml, &base_dir, &fungi_home, &policy)
            .await
    }

    pub async fn start_service(&self, runtime: RuntimeKind, handle: String) -> Result<()> {
        self.runtime_control().start(runtime, &handle).await?;
        self.sync_service_endpoint_listeners_by_handle(&handle, true)
            .await
    }

    pub async fn start_service_by_handle(&self, handle: String) -> Result<()> {
        self.runtime_control().start_by_handle(&handle).await?;
        self.sync_service_endpoint_listeners_by_handle(&handle, true)
            .await
    }

    pub async fn stop_service(&self, runtime: RuntimeKind, handle: String) -> Result<()> {
        self.runtime_control().stop(runtime, &handle).await?;
        self.sync_service_endpoint_listeners_by_handle(&handle, false)
            .await
    }

    pub async fn stop_service_by_handle(&self, handle: String) -> Result<()> {
        self.runtime_control().stop_by_handle(&handle).await?;
        self.sync_service_endpoint_listeners_by_handle(&handle, false)
            .await
    }

    pub async fn remove_service(&self, runtime: RuntimeKind, handle: String) -> Result<()> {
        let manifest = self.runtime_control().get_service_manifest(&handle);
        self.runtime_control().remove(runtime, &handle).await?;
        self.sync_service_endpoint_listeners_for_manifest(manifest.as_ref(), false)
            .await
    }

    pub async fn remove_service_by_handle(&self, handle: String) -> Result<()> {
        let manifest = self.runtime_control().get_service_manifest(&handle);
        self.runtime_control().remove_by_handle(&handle).await?;
        self.sync_service_endpoint_listeners_for_manifest(manifest.as_ref(), false)
            .await
    }

    pub async fn inspect_service(
        &self,
        runtime: RuntimeKind,
        handle: String,
    ) -> Result<ServiceInstance> {
        self.runtime_control().inspect(runtime, &handle).await
    }

    pub async fn inspect_service_by_handle(&self, handle: String) -> Result<ServiceInstance> {
        self.runtime_control().inspect_by_handle(&handle).await
    }

    pub async fn get_service_logs(
        &self,
        runtime: RuntimeKind,
        handle: String,
        tail: Option<String>,
    ) -> Result<ServiceLogs> {
        self.runtime_control()
            .logs(runtime, &handle, &ServiceLogsOptions { tail })
            .await
    }

    pub async fn get_service_logs_by_handle(
        &self,
        handle: String,
        tail: Option<String>,
    ) -> Result<ServiceLogs> {
        self.runtime_control()
            .logs_by_handle(&handle, &ServiceLogsOptions { tail })
            .await
    }

    pub async fn list_services(&self) -> Result<Vec<ServiceInstance>> {
        self.runtime_control().list_services().await
    }

    pub async fn list_exposed_services(&self) -> Result<Vec<DiscoveredService>> {
        self.runtime_control().list_exposed_services().await
    }

    pub async fn discover_peer_services(&self, peer_id: PeerId) -> Result<Vec<DiscoveredService>> {
        self.service_discovery_control()
            .discover_peer_services(peer_id)
            .await
    }

    pub fn local_node_capabilities(&self) -> NodeCapabilities {
        let config = self.config().lock().clone();
        build_local_node_capabilities(&config, self.runtime_control())
    }

    pub async fn discover_peer_capabilities(&self, peer_id: PeerId) -> Result<NodeCapabilities> {
        self.node_capabilities_control()
            .discover_peer_capabilities(peer_id)
            .await
    }

    pub async fn remote_deploy_service(
        &self,
        peer_id: PeerId,
        manifest_yaml: String,
    ) -> Result<ServiceControlResponse> {
        self.service_control_protocol_control()
            .deploy_peer_service(peer_id, manifest_yaml)
            .await
    }

    pub async fn remote_start_service(
        &self,
        peer_id: PeerId,
        handle: String,
    ) -> Result<ServiceControlResponse> {
        self.service_control_protocol_control()
            .start_peer_service(peer_id, handle)
            .await
    }

    pub async fn remote_list_services(&self, peer_id: PeerId) -> Result<ServiceControlResponse> {
        self.service_control_protocol_control()
            .list_peer_services(peer_id)
            .await
    }

    pub async fn remote_stop_service(
        &self,
        peer_id: PeerId,
        handle: String,
    ) -> Result<ServiceControlResponse> {
        let response = self
            .service_control_protocol_control()
            .stop_peer_service(peer_id, handle)
            .await?;
        let service_key = response
            .service
            .as_ref()
            .map(|service| service.name.as_str())
            .unwrap_or_default()
            .to_string();
        if !service_key.is_empty() {
            let _ = self.disable_remote_service_by_match(peer_id, &service_key);
        }
        Ok(response)
    }

    pub async fn remote_remove_service(
        &self,
        peer_id: PeerId,
        handle: String,
    ) -> Result<ServiceControlResponse> {
        let response = self
            .service_control_protocol_control()
            .remove_peer_service(peer_id, handle)
            .await?;
        let service_key = response
            .service
            .as_ref()
            .map(|service| service.name.as_str())
            .unwrap_or_default()
            .to_string();
        if !service_key.is_empty() {
            let _ = self.disable_remote_service_by_match(peer_id, &service_key);
        }
        Ok(response)
    }

    pub async fn mdns_get_local_devices(&self) -> Result<Vec<PeerInfo>> {
        let local_devices = self
            .mdns_control()
            .get_all_devices()
            .values()
            .cloned()
            .collect();
        Ok(local_devices)
    }

    pub fn address_book_get_all(&self) -> Vec<PeerInfo> {
        self.address_book().lock().get_all_peers().clone()
    }

    pub fn address_book_add_or_update(&self, peer_info: PeerInfo) -> Result<()> {
        let current_peers_config = self.address_book().lock().clone();
        let updated_peers_config = current_peers_config.add_or_update_peer(peer_info)?;
        *self.address_book().lock() = updated_peers_config;
        Ok(())
    }

    pub fn address_book_get_peer(&self, peer_id: PeerId) -> Option<PeerInfo> {
        self.address_book().lock().get_peer_info(&peer_id).cloned()
    }

    pub fn address_book_remove(&self, peer_id: PeerId) -> Result<()> {
        let current_peers_config = self.address_book().lock().clone();
        let updated_peers_config = current_peers_config.remove_peer(&peer_id)?;
        *self.address_book().lock() = updated_peers_config;
        Ok(())
    }

    pub fn get_incoming_allowed_peers(&self) -> Vec<PeerInfo> {
        let allowed_peers = self
            .swarm_control()
            .state()
            .get_incoming_allowed_peers_list();
        let peers_config_guard = self.address_book();
        let peers_config = peers_config_guard.lock();

        allowed_peers
            .into_iter()
            .map(
                |peer_id| match peers_config.get_peer_info(&peer_id).cloned() {
                    Some(peer_info) => peer_info,
                    None => PeerInfo::new_unknown(peer_id),
                },
            )
            .collect()
    }
}

fn allocate_local_forward_port(preferred_port: u16, reserved_ports: &BTreeSet<u16>) -> Result<u16> {
    if preferred_port != 0
        && !reserved_ports.contains(&preferred_port)
        && StdTcpListener::bind(("127.0.0.1", preferred_port)).is_ok()
    {
        return Ok(preferred_port);
    }

    for _ in 0..32 {
        let listener = StdTcpListener::bind(("127.0.0.1", 0))?;
        let port = listener.local_addr()?.port();
        if !reserved_ports.contains(&port) {
            return Ok(port);
        }
    }

    bail!("failed to allocate a free local TCP port for remote service forwarding")
}
