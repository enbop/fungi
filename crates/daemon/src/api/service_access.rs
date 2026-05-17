use std::{
    collections::{BTreeMap, BTreeSet},
    net::TcpListener as StdTcpListener,
};

use anyhow::{Result, bail};
use fungi_config::{
    local_access::{LocalAccessConfig, LocalAccessRecord, LocalPortSource},
    tcp_tunneling::ForwardingRule,
};
use libp2p::PeerId;

use crate::{CatalogServiceEndpoint, FungiDaemon};

use super::types::{ServiceAccess, ServiceAccessEndpoint};

impl FungiDaemon {
    pub fn get_service_access_forwarding_rules(&self) -> Vec<(String, ForwardingRule)> {
        self.tcp_tunneling_control().get_forwarding_rules()
    }

    pub fn get_service_endpoint_listening_rules(
        &self,
    ) -> Vec<(String, fungi_config::tcp_tunneling::ListeningRule)> {
        self.tcp_tunneling_control().get_listening_rules()
    }

    async fn start_service_access_forwarding_rule(
        &self,
        record: &LocalAccessRecord,
        remote_protocol: String,
        remote_port: u16,
    ) -> Result<()> {
        let rule = ForwardingRule {
            local_host: record.local_host.clone(),
            local_port: record.local_port,
            remote_peer_id: record.remote_peer_id.clone(),
            remote_protocol: Some(remote_protocol),
            remote_port,
            remote_service_id: None,
            remote_service_name: Some(record.remote_service_name.clone()),
            remote_service_port_name: Some(record.remote_service_port_name.clone()),
        };
        self.add_service_access_forwarding_rule_internal(rule)
            .await?;
        Ok(())
    }

    pub async fn attach_service_access(
        &self,
        peer_id: PeerId,
        service_name: String,
        entry: Option<String>,
        local_port: Option<u16>,
    ) -> Result<ServiceAccess> {
        let catalog_services = self.list_peer_catalog(peer_id).await?;
        let service = catalog_services
            .into_iter()
            .find(|service| service.service_name == service_name)
            .ok_or_else(|| anyhow::anyhow!("remote service not found: {}", service_name))?;

        if service.endpoints.is_empty() {
            bail!(
                "remote service exposes no named TCP endpoints: {}",
                service.service_name
            );
        }

        if local_port.is_some() && service.endpoints.len() > 1 && entry.is_none() {
            bail!("choose a service entry before assigning a fixed local port");
        }

        let peer_id_string = peer_id.to_string();
        let mut access_config = self.local_access_config()?;
        let mut active_rules = self.get_service_access_forwarding_rules();
        let mut reserved_local_ports = access_config
            .records
            .iter()
            .map(|record| record.local_port)
            .chain(active_rules.iter().map(|(_, rule)| rule.local_port))
            .collect::<BTreeSet<_>>();
        let mut enabled_endpoints = Vec::new();
        let endpoints = service
            .endpoints
            .into_iter()
            .filter(|endpoint| {
                entry
                    .as_deref()
                    .map(|entry| endpoint.name == entry)
                    .unwrap_or(true)
            })
            .collect::<Vec<_>>();

        if endpoints.is_empty() {
            let entry = entry.unwrap_or_else(|| "default".to_string());
            bail!("remote service entry not found: {}", entry);
        }

        for endpoint in endpoints {
            let remote_port = catalog_endpoint_listen_port(&endpoint);
            let existing_record = access_config
                .find_record(&peer_id_string, &service.service_name, &endpoint.name)
                .cloned();
            let existing_active_rule = find_active_rule(
                &active_rules,
                &peer_id_string,
                &service.service_name,
                &endpoint.name,
            );

            if let Some(record) = &existing_record {
                reserved_local_ports.remove(&record.local_port);
            }
            if let Some((_, rule)) = &existing_active_rule {
                reserved_local_ports.remove(&rule.local_port);
            }

            let selected_local_port = match (local_port, existing_record.as_ref()) {
                (Some(local_port), _) => local_port,
                (None, Some(record)) => record.local_port,
                (None, None) => {
                    allocate_local_access_port(endpoint.service_port, &reserved_local_ports)?
                }
            };
            let local_port_source = if local_port.is_some() {
                LocalPortSource::User
            } else {
                existing_record
                    .as_ref()
                    .map(|record| record.local_port_source)
                    .unwrap_or_default()
            };

            let active_rule_matches = existing_active_rule.as_ref().is_some_and(|(_, rule)| {
                rule.local_host == "127.0.0.1"
                    && rule.local_port == selected_local_port
                    && rule.remote_port == remote_port
                    && rule.remote_protocol.as_deref() == Some(endpoint.protocol.as_str())
            });
            let active_rule_owns_selected_port =
                existing_active_rule.as_ref().is_some_and(|(_, rule)| {
                    rule.local_host == "127.0.0.1" && rule.local_port == selected_local_port
                });

            if !active_rule_matches && !active_rule_owns_selected_port {
                ensure_local_port_available(selected_local_port, &reserved_local_ports)?;
            }

            let record = LocalAccessRecord {
                remote_peer_id: peer_id_string.clone(),
                remote_service_name: service.service_name.clone(),
                remote_service_port_name: endpoint.name.clone(),
                local_host: "127.0.0.1".to_string(),
                local_port: selected_local_port,
                local_port_source,
                last_remote_protocol: Some(endpoint.protocol.clone()),
                last_remote_port: Some(remote_port),
            };

            access_config = access_config.upsert_record(record.clone())?;

            if let Some((rule_id, _)) = existing_active_rule
                && !active_rule_matches
            {
                self.remove_service_access_forwarding_rule_internal(&rule_id)?;
                active_rules.retain(|(existing_rule_id, _)| existing_rule_id != &rule_id);
            }

            if !active_rule_matches {
                self.start_service_access_forwarding_rule(
                    &record,
                    endpoint.protocol.clone(),
                    remote_port,
                )
                .await?;
                active_rules = self.get_service_access_forwarding_rules();
            }

            reserved_local_ports.insert(selected_local_port);
            enabled_endpoints.push(ServiceAccessEndpoint {
                name: endpoint.name,
                protocol: endpoint.protocol,
                local_host: record.local_host,
                local_port: record.local_port,
                remote_port,
            });
        }

        enabled_endpoints.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(ServiceAccess {
            peer_id: peer_id_string,
            service_name: service.service_name,
            endpoints: enabled_endpoints,
        })
    }

    pub fn detach_service_access(&self, peer_id: PeerId, service_name: String) -> Result<()> {
        self.detach_service_access_by_match(peer_id, &service_name)
    }

    pub fn detach_service_access_by_match(&self, peer_id: PeerId, matcher: &str) -> Result<()> {
        let peer_id_string = peer_id.to_string();
        let rules_to_remove = self
            .get_service_access_forwarding_rules()
            .into_iter()
            .filter(|(_, rule)| {
                rule.remote_peer_id == peer_id_string
                    && rule.remote_service_name.as_deref() == Some(matcher)
            })
            .map(|(rule_id, _)| rule_id)
            .collect::<Vec<_>>();

        for rule_id in rules_to_remove {
            self.remove_service_access_forwarding_rule_internal(&rule_id)?;
        }

        Ok(())
    }

    pub fn forget_service_access(&self, peer_id: PeerId, service_name: String) -> Result<()> {
        self.detach_service_access_by_match(peer_id, &service_name)?;
        let peer_id_string = peer_id.to_string();
        self.local_access_config()?
            .remove_service_records(&peer_id_string, &service_name)?;
        Ok(())
    }

    pub fn forget_device_service_accesses(&self, peer_id: PeerId) -> Result<()> {
        let peer_id_string = peer_id.to_string();
        let rules_to_remove = self
            .get_service_access_forwarding_rules()
            .into_iter()
            .filter(|(_, rule)| rule.remote_peer_id == peer_id_string)
            .map(|(rule_id, _)| rule_id)
            .collect::<Vec<_>>();

        for rule_id in rules_to_remove {
            self.remove_service_access_forwarding_rule_internal(&rule_id)?;
        }

        self.local_access_config()?
            .remove_device_records(&peer_id_string)?;
        Ok(())
    }

    pub fn list_service_accesses(&self, peer_id: Option<PeerId>) -> Result<Vec<ServiceAccess>> {
        let peer_filter = peer_id.map(|peer_id| peer_id.to_string());
        let mut grouped = BTreeMap::<(String, String), Vec<ServiceAccessEndpoint>>::new();

        for record in self.local_access_config()?.records {
            if let Some(peer_filter) = &peer_filter
                && &record.remote_peer_id != peer_filter
            {
                continue;
            }

            grouped
                .entry((
                    record.remote_peer_id.clone(),
                    record.remote_service_name.clone(),
                ))
                .or_default()
                .push(ServiceAccessEndpoint {
                    name: record.remote_service_port_name,
                    protocol: record.last_remote_protocol.unwrap_or_default(),
                    local_host: record.local_host,
                    local_port: record.local_port,
                    remote_port: record.last_remote_port.unwrap_or_default(),
                });
        }

        let mut services = grouped
            .into_iter()
            .map(|((peer_id, service_name), mut endpoints)| {
                endpoints.sort_by(|left, right| left.name.cmp(&right.name));
                ServiceAccess {
                    peer_id,
                    service_name,
                    endpoints,
                }
            })
            .collect::<Vec<_>>();
        services.sort_by(|left, right| {
            left.peer_id
                .cmp(&right.peer_id)
                .then(left.service_name.cmp(&right.service_name))
        });
        Ok(services)
    }

    fn local_access_config(&self) -> Result<LocalAccessConfig> {
        let fungi_dir = self.config_fungi_dir()?;
        LocalAccessConfig::apply_from_dir(&fungi_dir)
    }
}

fn find_active_rule(
    active_rules: &[(String, ForwardingRule)],
    remote_peer_id: &str,
    remote_service_name: &str,
    remote_service_port_name: &str,
) -> Option<(String, ForwardingRule)> {
    active_rules
        .iter()
        .find(|(_, rule)| {
            rule.remote_peer_id == remote_peer_id
                && rule.remote_service_name.as_deref() == Some(remote_service_name)
                && rule.remote_service_port_name.as_deref() == Some(remote_service_port_name)
        })
        .cloned()
}

fn catalog_endpoint_listen_port(endpoint: &CatalogServiceEndpoint) -> u16 {
    if endpoint.host_port == 0 {
        endpoint.service_port
    } else {
        endpoint.host_port
    }
}

fn ensure_local_port_available(port: u16, reserved_ports: &BTreeSet<u16>) -> Result<()> {
    if reserved_ports.contains(&port) {
        bail!(
            "local port is already reserved by another service access: {}",
            port
        );
    }
    StdTcpListener::bind(("127.0.0.1", port))
        .map(|_| ())
        .map_err(|error| anyhow::anyhow!("local port {} is not available: {}", port, error))
}

fn allocate_local_access_port(preferred_port: u16, reserved_ports: &BTreeSet<u16>) -> Result<u16> {
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

    bail!("failed to allocate a free local TCP port for remote service access")
}
