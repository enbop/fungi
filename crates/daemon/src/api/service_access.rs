#[cfg(test)]
use std::{collections::BTreeMap, net::TcpListener as StdTcpListener};

use anyhow::Result;
use fungi_config::tcp_tunneling::ForwardingRule;
use libp2p::PeerId;

use crate::{FungiControl, service_accesses::restore_saved_service_accesses};

use super::types::ServiceAccess;

impl FungiControl {
    pub fn get_service_access_forwarding_rules(&self) -> Vec<(String, ForwardingRule)> {
        self.service_access().forwarding_rules()
    }

    pub fn get_service_endpoint_listening_rules(
        &self,
    ) -> Vec<(String, fungi_config::tcp_tunneling::ListeningRule)> {
        self.services().tcp_tunneling().get_listening_rules()
    }

    pub async fn attach_service_access(
        &self,
        peer_id: PeerId,
        service_name: String,
        entry: Option<String>,
        local_port: Option<u16>,
    ) -> Result<ServiceAccess> {
        anyhow::ensure!(
            peer_id != self.devices().local().id(),
            "local services do not use remote service access listeners"
        );
        let service = self
            .services()
            .for_device(peer_id)
            .service(service_name)
            .inspect()
            .await
            .map_err(|error| {
                anyhow::anyhow!("failed to refresh remote service before attaching access: {error}")
            })?;
        self.service_access()
            .attach(peer_id, service, entry, local_port)
            .await
    }

    pub async fn restore_saved_service_access_from_snapshots(&self) {
        restore_saved_service_accesses(self.service_access().clone(), self.services().clone())
            .await;
    }

    pub fn detach_service_access(&self, peer_id: PeerId, service_name: String) -> Result<()> {
        self.service_access().detach(peer_id, &service_name)
    }

    pub async fn restore_saved_service_access(
        &self,
        peer_id: PeerId,
        service_name: String,
    ) -> Result<()> {
        let saved_entries = self
            .service_access()
            .saved_entries(peer_id, &service_name)
            .await?;
        if saved_entries.is_empty() {
            return Ok(());
        }
        let service = self
            .services()
            .for_device(peer_id)
            .service(service_name)
            .inspect()
            .await?;
        for entry in saved_entries {
            self.service_access()
                .attach(peer_id, service.clone(), Some(entry), None)
                .await?;
        }
        Ok(())
    }

    pub fn detach_service_access_by_match(&self, peer_id: PeerId, matcher: &str) -> Result<()> {
        self.service_access().detach(peer_id, matcher)
    }

    pub async fn forget_service_access(&self, peer_id: PeerId, service_name: String) -> Result<()> {
        self.service_access()
            .forget_service(peer_id, &service_name)
            .await
    }

    pub async fn forget_device_service_accesses(&self, peer_id: PeerId) -> Result<()> {
        self.service_access().forget_device(peer_id).await
    }

    pub async fn list_service_accesses(
        &self,
        peer_id: Option<PeerId>,
    ) -> Result<Vec<ServiceAccess>> {
        self.service_access().list(peer_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        RuntimeKind, ServiceExpose, ServiceExposeTransport, ServiceExposeTransportKind,
        ServiceExposeUsage, ServiceExposeUsageKind, ServiceManifest, ServiceMount, ServicePort,
        ServicePortAllocation, ServicePortProtocol, ServiceSource,
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
    async fn stale_restore_does_not_recreate_forgotten_listener() -> Result<()> {
        let service_name = "forgotten-restore";
        let (client, server) =
            setup_access_test_pair(service_name, vec![("main", free_tcp_port()?)]).await?;
        let peer_id = server.peer_id();

        client
            .daemon()
            .attach_service_access(
                peer_id,
                service_name.to_string(),
                Some("main".to_string()),
                None,
            )
            .await?;
        let stale_records = client
            .daemon()
            .service_access()
            .preference_records()
            .await?;

        client
            .daemon()
            .forget_service_access(peer_id, service_name.to_string())
            .await?;
        client
            .daemon()
            .service_access()
            .restore_records_from_cached_snapshots(&stale_records, client.daemon().services())
            .await;

        assert!(
            client
                .daemon()
                .get_service_access_forwarding_rules()
                .is_empty()
        );
        assert!(
            client
                .daemon()
                .list_service_accesses(Some(peer_id))
                .await?
                .is_empty()
        );
        Ok(())
    }

    #[tokio::test]
    async fn stale_restore_preserves_reattached_listener() -> Result<()> {
        let service_name = "reattached-restore";
        let first_local_port = free_tcp_port()?;
        let mut second_local_port = free_tcp_port()?;
        while second_local_port == first_local_port {
            second_local_port = free_tcp_port()?;
        }
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
        let stale_records = client
            .daemon()
            .service_access()
            .preference_records()
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
        client
            .daemon()
            .service_access()
            .restore_records_from_cached_snapshots(&stale_records, client.daemon().services())
            .await;

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

    #[tokio::test]
    async fn stale_restore_preserves_explicit_detach_until_reattach() -> Result<()> {
        let service_name = "detached-restore";
        let local_port = free_tcp_port()?;
        let (client, server) =
            setup_access_test_pair(service_name, vec![("main", free_tcp_port()?)]).await?;
        let peer_id = server.peer_id();

        client
            .daemon()
            .attach_service_access(
                peer_id,
                service_name.to_string(),
                Some("main".to_string()),
                Some(local_port),
            )
            .await?;
        let stale_records = client
            .daemon()
            .service_access()
            .preference_records()
            .await?;

        client
            .daemon()
            .detach_service_access(peer_id, service_name.to_string())?;
        client
            .daemon()
            .service_access()
            .restore_records_from_cached_snapshots(&stale_records, client.daemon().services())
            .await;

        assert!(
            client
                .daemon()
                .get_service_access_forwarding_rules()
                .is_empty()
        );
        assert_eq!(
            client
                .daemon()
                .list_service_accesses(Some(peer_id))
                .await?
                .len(),
            1
        );

        client
            .daemon()
            .attach_service_access(
                peer_id,
                service_name.to_string(),
                Some("main".to_string()),
                None,
            )
            .await?;
        assert_eq!(
            client.daemon().get_service_access_forwarding_rules().len(),
            1
        );
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

    #[tokio::test]
    async fn attach_rejects_cached_snapshot_when_live_refresh_fails() -> Result<()> {
        let service_name = "cached-only";
        let (client, server) =
            setup_access_test_pair(service_name, vec![("main", free_tcp_port()?)]).await?;
        let peer_id = server.peer_id();

        client
            .daemon()
            .attach_service_access(
                peer_id,
                service_name.to_string(),
                Some("main".to_string()),
                None,
            )
            .await?;

        server.daemon().untrust_device(client.peer_id())?;

        let error = client
            .daemon()
            .attach_service_access(
                peer_id,
                service_name.to_string(),
                Some("main".to_string()),
                None,
            )
            .await
            .expect_err("cached service metadata must not make an unavailable service attachable");

        assert!(
            error
                .to_string()
                .contains("failed to refresh remote service before attaching access")
        );
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
