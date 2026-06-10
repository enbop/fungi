use std::{
    collections::{BTreeMap, BTreeSet},
    net::TcpListener as StdTcpListener,
};

use anyhow::{Result, bail};
use fungi_config::{
    local_preferences::{LocalPortSource, LocalPreferenceCache, LocalServicePreference},
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
        record: &LocalServicePreference,
        remote_protocol: String,
        remote_port: u16,
    ) -> Result<String> {
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
        self.add_service_access_forwarding_rule_internal(rule).await
    }

    async fn restore_service_access_forwarding_rule(&self, rule: ForwardingRule) {
        if let Err(error) = self.add_service_access_forwarding_rule_internal(rule).await {
            log::warn!(
                "Failed to restore previous service access listener after attach failure: {}",
                error
            );
        }
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

        let local_preferences_lock = self.local_preferences_lock();
        let _local_preferences_guard = local_preferences_lock.lock().await;

        let peer_id_string = peer_id.to_string();
        let mut local_preferences = self.local_preferences()?;
        let mut active_rules = self.get_service_access_forwarding_rules();
        let mut reserved_local_ports = local_preferences
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
            let existing_record = local_preferences
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
                    allocate_preferred_local_port(endpoint.service_port, &reserved_local_ports)?
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

            let record = LocalServicePreference {
                remote_peer_id: peer_id_string.clone(),
                remote_service_name: service.service_name.clone(),
                remote_service_port_name: endpoint.name.clone(),
                local_host: "127.0.0.1".to_string(),
                local_port: selected_local_port,
                local_port_source,
            };

            let updated_local_preferences =
                local_preferences.with_upserted_record(record.clone())?;

            let mut removed_active_rule = None;
            let mut started_rule_id = None;
            if !active_rule_matches {
                if let Some((rule_id, rule)) = existing_active_rule {
                    self.remove_service_access_forwarding_rule_internal(&rule_id)?;
                    removed_active_rule = Some(rule);
                }

                match self
                    .start_service_access_forwarding_rule(
                        &record,
                        endpoint.protocol.clone(),
                        remote_port,
                    )
                    .await
                {
                    Ok(rule_id) => started_rule_id = Some(rule_id),
                    Err(error) => {
                        if let Some(rule) = removed_active_rule {
                            self.restore_service_access_forwarding_rule(rule).await;
                        }
                        return Err(error);
                    }
                }
            }

            if let Err(error) = updated_local_preferences.save_to_file() {
                if let Some(rule_id) = started_rule_id {
                    if let Err(rollback_error) =
                        self.remove_service_access_forwarding_rule_internal(&rule_id)
                    {
                        log::warn!(
                            "Failed to roll back service access listener after save failure: {}",
                            rollback_error
                        );
                    }
                }
                if let Some(rule) = removed_active_rule {
                    self.restore_service_access_forwarding_rule(rule).await;
                }
                return Err(error);
            }

            local_preferences = updated_local_preferences;

            if !active_rule_matches {
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

    pub async fn restore_saved_service_access(
        &self,
        peer_id: PeerId,
        service_name: String,
    ) -> Result<()> {
        let peer_id_string = peer_id.to_string();
        let saved_entries = {
            let local_preferences_lock = self.local_preferences_lock();
            let _local_preferences_guard = local_preferences_lock.lock().await;
            self.local_preferences()?
                .records
                .into_iter()
                .filter(|record| {
                    record.remote_peer_id == peer_id_string
                        && record.remote_service_name == service_name
                })
                .map(|record| record.remote_service_port_name)
                .collect::<BTreeSet<_>>()
        };

        for entry in saved_entries {
            self.attach_service_access(peer_id, service_name.clone(), Some(entry), None)
                .await?;
        }

        Ok(())
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

    pub async fn forget_service_access(&self, peer_id: PeerId, service_name: String) -> Result<()> {
        let local_preferences_lock = self.local_preferences_lock();
        let _local_preferences_guard = local_preferences_lock.lock().await;

        self.detach_service_access_by_match(peer_id, &service_name)?;
        let peer_id_string = peer_id.to_string();
        self.local_preferences()?
            .remove_service_records(&peer_id_string, &service_name)?;
        Ok(())
    }

    pub async fn forget_device_service_accesses(&self, peer_id: PeerId) -> Result<()> {
        let local_preferences_lock = self.local_preferences_lock();
        let _local_preferences_guard = local_preferences_lock.lock().await;

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

        self.local_preferences()?
            .remove_device_records(&peer_id_string)?;
        Ok(())
    }

    pub async fn list_service_accesses(
        &self,
        peer_id: Option<PeerId>,
    ) -> Result<Vec<ServiceAccess>> {
        let local_preferences_lock = self.local_preferences_lock();
        let _local_preferences_guard = local_preferences_lock.lock().await;

        let peer_filter = peer_id.map(|peer_id| peer_id.to_string());
        let mut grouped = BTreeMap::<(String, String), Vec<ServiceAccessEndpoint>>::new();

        for record in self.local_preferences()?.records {
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
                    protocol: String::new(),
                    local_host: record.local_host,
                    local_port: record.local_port,
                    remote_port: 0,
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

    fn local_preferences(&self) -> Result<LocalPreferenceCache> {
        let fungi_dir = self.config_fungi_dir()?;
        LocalPreferenceCache::apply_from_dir(&fungi_dir)
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

fn allocate_preferred_local_port(
    preferred_port: u16,
    reserved_ports: &BTreeSet<u16>,
) -> Result<u16> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        RuntimeKind, ServiceExpose, ServiceExposeTransport, ServiceExposeTransportKind,
        ServiceExposeUsage, ServiceExposeUsageKind, ServiceManifest, ServiceMount, ServicePort,
        ServicePortAllocation, ServicePortProtocol, ServiceRunMode, ServiceSource,
        test_support::{TestDaemon, spawn_connected_pair},
    };
    use libp2p::swarm::dial_opts::DialOpts;
    use std::time::Duration;

    #[tokio::test]
    async fn concurrent_attach_accesses_preserve_both_saved_records() -> Result<()> {
        let service_name = "multi-access";
        let entries = vec![("api", free_tcp_port()?), ("metrics", free_tcp_port()?)];
        let (client, server) = setup_access_test_pair(service_name, entries).await?;
        let peer_id = server.peer_id();

        let attach_api = client.daemon().attach_service_access(
            peer_id,
            service_name.to_string(),
            Some("api".to_string()),
            None,
        );
        let attach_metrics = client.daemon().attach_service_access(
            peer_id,
            service_name.to_string(),
            Some("metrics".to_string()),
            None,
        );
        let (api_access, metrics_access) = tokio::join!(attach_api, attach_metrics);
        api_access?;
        metrics_access?;

        let accesses = client.daemon().list_service_accesses(Some(peer_id)).await?;
        let saved = accesses
            .iter()
            .find(|access| access.service_name == service_name)
            .expect("expected saved service access");
        let endpoint_names = saved
            .endpoints
            .iter()
            .map(|endpoint| endpoint.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(endpoint_names, vec!["api", "metrics"]);
        Ok(())
    }

    #[tokio::test]
    async fn concurrent_attach_and_forget_keep_local_preferences_readable() -> Result<()> {
        let service_name = "forget-race";
        let (client, server) =
            setup_access_test_pair(service_name, vec![("main", free_tcp_port()?)]).await?;
        let peer_id = server.peer_id();

        let attach = client.daemon().attach_service_access(
            peer_id,
            service_name.to_string(),
            Some("main".to_string()),
            None,
        );
        let forget = client
            .daemon()
            .forget_service_access(peer_id, service_name.to_string());
        let (attach_result, forget_result) = tokio::join!(attach, forget);
        attach_result?;
        forget_result?;

        let accesses = client.daemon().list_service_accesses(Some(peer_id)).await?;
        let saved_count = accesses
            .iter()
            .filter(|access| access.service_name == service_name)
            .count();
        assert!(saved_count <= 1);
        Ok(())
    }

    #[tokio::test]
    async fn attach_with_new_local_port_updates_saved_preference_and_listener() -> Result<()> {
        let service_name = "move-port";
        let first_local_port = free_tcp_port()?;
        let second_local_port = free_tcp_port()?;
        let (client, server) =
            setup_access_test_pair(service_name, vec![("main", free_tcp_port()?)]).await?;
        let peer_id = server.peer_id();

        client
            .daemon()
            .attach_service_access(
                peer_id,
                service_name.to_string(),
                Some("main".to_string()),
                Some(first_local_port),
            )
            .await?;
        client
            .daemon()
            .attach_service_access(
                peer_id,
                service_name.to_string(),
                Some("main".to_string()),
                Some(second_local_port),
            )
            .await?;

        let accesses = client.daemon().list_service_accesses(Some(peer_id)).await?;
        let saved = accesses
            .iter()
            .find(|access| access.service_name == service_name)
            .and_then(|access| {
                access
                    .endpoints
                    .iter()
                    .find(|endpoint| endpoint.name == "main")
            })
            .expect("expected saved local address");
        assert_eq!(saved.local_port, second_local_port);

        let active_ports = client
            .daemon()
            .get_service_access_forwarding_rules()
            .into_iter()
            .filter(|(_, rule)| rule.remote_service_name.as_deref() == Some(service_name))
            .map(|(_, rule)| rule.local_port)
            .collect::<Vec<_>>();
        assert_eq!(active_ports, vec![second_local_port]);
        Ok(())
    }

    async fn setup_access_test_pair(
        service_name: &str,
        entries: Vec<(&str, u16)>,
    ) -> Result<(TestDaemon, TestDaemon)> {
        let (client, server) = spawn_connected_pair().await?;
        server
            .daemon()
            .pull_service(exposed_external_manifest(service_name, entries))
            .await?;
        server
            .daemon()
            .start_service_by_name(service_name.to_string())
            .await?;
        let server_peer_id = server.peer_id();
        let server_addr = server.tcp_multiaddr();
        client
            .swarm_control()
            .invoke_swarm(move |swarm| {
                swarm.dial(
                    DialOpts::peer_id(server_peer_id)
                        .addresses(vec![server_addr])
                        .build(),
                )
            })
            .await??;
        client
            .wait_connected(server.peer_id(), Duration::from_secs(5))
            .await?;
        server
            .wait_connected(client.peer_id(), Duration::from_secs(5))
            .await?;
        Ok((client, server))
    }

    fn exposed_external_manifest(service_name: &str, entries: Vec<(&str, u16)>) -> ServiceManifest {
        let first_port = entries
            .first()
            .map(|(_, port)| *port)
            .expect("test manifest requires at least one port");
        ServiceManifest {
            name: service_name.to_string(),
            definition_id: None,
            runtime: RuntimeKind::External,
            run_mode: ServiceRunMode::Command,
            source: ServiceSource::ExistingTcp {
                host: "127.0.0.1".to_string(),
                port: first_port,
            },
            expose: Some(ServiceExpose {
                transport: ServiceExposeTransport {
                    kind: ServiceExposeTransportKind::Tcp,
                },
                usage: Some(ServiceExposeUsage {
                    kind: ServiceExposeUsageKind::Raw,
                    path: None,
                }),
                icon_url: None,
                catalog_id: None,
            }),
            env: BTreeMap::new(),
            mounts: Vec::<ServiceMount>::new(),
            ports: entries
                .into_iter()
                .map(|(name, port)| ServicePort {
                    name: Some(name.to_string()),
                    host_port: port,
                    host_port_allocation: ServicePortAllocation::Fixed,
                    service_port: port,
                    protocol: ServicePortProtocol::Tcp,
                })
                .collect(),
            command: Vec::new(),
            entrypoint: Vec::new(),
            working_dir: None,
            labels: BTreeMap::new(),
        }
    }

    fn free_tcp_port() -> Result<u16> {
        let listener = StdTcpListener::bind(("127.0.0.1", 0))?;
        Ok(listener.local_addr()?.port())
    }
}
