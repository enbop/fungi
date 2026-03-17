use std::{
    collections::{BTreeMap, BTreeSet},
    net::TcpListener as StdTcpListener,
};

use anyhow::{Result, bail};
use libp2p::PeerId;

use crate::FungiDaemon;

use super::types::{ServiceAccess, ServiceAccessEndpoint};

impl FungiDaemon {
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

    pub async fn attach_service_access(
        &self,
        peer_id: PeerId,
        service_id: String,
    ) -> Result<ServiceAccess> {
        let catalog_services = self.list_peer_catalog(peer_id).await?;
        let service = catalog_services
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
                enabled_endpoints.push(ServiceAccessEndpoint {
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

            self.add_tcp_forwarding_rule_with_details(
                "127.0.0.1".to_string(),
                local_port,
                peer_id_string.clone(),
                0,
                Some(endpoint.protocol.clone()),
                Some(service.service_id.clone()),
                Some(service.service_name.clone()),
                Some(endpoint.name.clone()),
            )
            .await?;

            enabled_endpoints.push(ServiceAccessEndpoint {
                name: endpoint.name,
                protocol: endpoint.protocol,
                local_host: "127.0.0.1".to_string(),
                local_port,
            });
        }

        enabled_endpoints.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(ServiceAccess {
            peer_id: peer_id_string,
            service_id: service.service_id,
            service_name: service.service_name,
            endpoints: enabled_endpoints,
        })
    }

    pub fn detach_service_access(&self, peer_id: PeerId, service_id: String) -> Result<()> {
        self.detach_service_access_by_match(peer_id, &service_id)
    }

    pub fn detach_service_access_by_match(&self, peer_id: PeerId, matcher: &str) -> Result<()> {
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

    pub fn list_service_accesses(&self, peer_id: Option<PeerId>) -> Vec<ServiceAccess> {
        let peer_filter = peer_id.map(|peer_id| peer_id.to_string());
        let mut grouped = BTreeMap::<(String, String, String), Vec<ServiceAccessEndpoint>>::new();

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
                .push(ServiceAccessEndpoint {
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
                ServiceAccess {
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
