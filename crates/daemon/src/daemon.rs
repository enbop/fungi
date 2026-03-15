use std::{
    env,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use crate::{
    DaemonArgs,
    controls::{
        DockerControl, FileTransferClientsControl, FileTransferServiceControl,
        NodeCapabilitiesControl, ServiceControlProtocolControl, ServiceDiscoveryControl,
        TcpTunnelingControl, mdns::MdnsControl,
    },
    runtime::RuntimeControl,
};
use anyhow::Result;
use fungi_config::{
    FungiConfig,
    address_book::{AddressBookConfig, PeerInfo},
    file_transfer::{FileTransferClient as FTCConfig, FileTransferService as FTSConfig},
};
use fungi_swarm::{FungiSwarm, State, SwarmControl, TSwarm};
use fungi_util::keypair::get_keypair_from_dir;
use libp2p::{Multiaddr, identity::Keypair, multiaddr::Protocol};
use parking_lot::Mutex;
use tokio::task::JoinHandle;

#[allow(dead_code)]
struct TaskHandles {
    swarm_task: JoinHandle<()>,
    proxy_ftp_task: Arc<Mutex<Option<JoinHandle<()>>>>,
    proxy_webdav_task: Arc<Mutex<Option<JoinHandle<()>>>>,
}

#[allow(dead_code)]
pub struct FungiDaemon {
    config: Arc<Mutex<FungiConfig>>,
    address_book_config: Arc<Mutex<AddressBookConfig>>,
    args: DaemonArgs,

    swarm_control: SwarmControl,
    mdns_control: MdnsControl,
    fts_control: FileTransferServiceControl,
    ftc_control: FileTransferClientsControl,
    docker_control: Option<DockerControl>,
    tcp_tunneling_control: TcpTunnelingControl,
    runtime_control: RuntimeControl,
    service_discovery_control: ServiceDiscoveryControl,
    node_capabilities_control: NodeCapabilitiesControl,
    service_control_protocol_control: ServiceControlProtocolControl,

    task_handles: TaskHandles,
}

impl FungiDaemon {
    pub fn config(&self) -> Arc<Mutex<FungiConfig>> {
        self.config.clone()
    }

    pub fn address_book(&self) -> Arc<Mutex<AddressBookConfig>> {
        self.address_book_config.clone()
    }

    pub fn swarm_control(&self) -> &SwarmControl {
        &self.swarm_control
    }

    pub fn fts_control(&self) -> &FileTransferServiceControl {
        &self.fts_control
    }

    pub fn ftc_control(&self) -> &FileTransferClientsControl {
        &self.ftc_control
    }

    pub fn docker_control(&self) -> Option<&DockerControl> {
        self.docker_control.as_ref()
    }

    pub fn tcp_tunneling_control(&self) -> &TcpTunnelingControl {
        &self.tcp_tunneling_control
    }

    pub fn runtime_control(&self) -> &RuntimeControl {
        &self.runtime_control
    }

    pub fn service_discovery_control(&self) -> &ServiceDiscoveryControl {
        &self.service_discovery_control
    }

    pub fn node_capabilities_control(&self) -> &NodeCapabilitiesControl {
        &self.node_capabilities_control
    }

    pub fn service_control_protocol_control(&self) -> &ServiceControlProtocolControl {
        &self.service_control_protocol_control
    }

    pub fn mdns_control(&self) -> &MdnsControl {
        &self.mdns_control
    }

    pub async fn start(fungi_dir: PathBuf, args: DaemonArgs) -> Result<Self> {
        println!("Fungi directory: {fungi_dir:?}");

        let config = FungiConfig::apply_from_dir(&fungi_dir)?;
        let keypair = get_keypair_from_dir(&fungi_dir)?;

        let address_book_config = AddressBookConfig::apply_from_dir(&fungi_dir)?;

        Self::start_with(args, config, keypair, address_book_config).await
    }

    pub async fn start_with(
        args: DaemonArgs,
        config: FungiConfig,
        keypair: Keypair,
        address_book_config: AddressBookConfig,
    ) -> Result<Self> {
        let state = State::new(
            config
                .network
                .incoming_allowed_peers
                .clone()
                .into_iter()
                .collect(),
        );

        let relay_addrs = match (
            config.network.disable_relay,
            config.network.custom_relay_addresses.is_empty(),
        ) {
            (true, _) => Vec::new(),
            (false, true) => fungi_swarm::get_default_relay_addrs(),
            (false, false) => config.network.custom_relay_addresses.clone(),
        };
        if relay_addrs.is_empty() {
            log::info!("Run without relay addresses");
        } else {
            for addr in &relay_addrs {
                log::info!("Using relay address: {addr}");
            }
        }

        let idle_connection_timeout =
            Duration::from_secs(config.network.idle_connection_timeout_secs.max(30));

        let (swarm_control, swarm_task) = FungiSwarm::start_swarm(
            keypair,
            state.clone(),
            relay_addrs,
            idle_connection_timeout,
            |swarm| {
                apply_listen(swarm, &config);
            },
        )
        .await?;
        let mdns_control = MdnsControl::new();
        let peer_info = PeerInfo::this_device(swarm_control.local_peer_id(), config.get_hostname());
        mdns_control.start(peer_info)?;

        let fts_control = FileTransferServiceControl::new(
            swarm_control.clone(),
            state.incoming_allowed_peers().clone(),
        );
        Self::init_fts(config.file_transfer.server.clone(), &fts_control).await;

        let ftc_control = FileTransferClientsControl::new(swarm_control.clone());
        Self::init_ftc(config.file_transfer.client.clone(), ftc_control.clone());

        let docker_control = DockerControl::from_config(&config.runtime)?;
        let shared_config = Arc::new(Mutex::new(config.clone()));
        let fungi_home = config
            .config_file_path()
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .to_path_buf();
        let runtime_root = config
            .config_file_path()
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join("runtime");
        let runtime_control = RuntimeControl::new(
            runtime_root,
            env::current_exe()
                .map_err(|e| anyhow::anyhow!("Failed to resolve current executable: {e}"))?,
            fungi_home.clone(),
            docker_control.clone(),
            fungi_home.join("services-state.json"),
            config.runtime.allowed_host_paths.clone(),
            config.runtime.wasmtime_enabled(),
        )?;
        runtime_control.restore_persisted_state().await?;
        let service_discovery_control = ServiceDiscoveryControl::new(
            swarm_control.clone(),
            runtime_control.clone(),
            state.incoming_allowed_peers().clone(),
        );
        service_discovery_control.start()?;
        let node_capabilities_control = NodeCapabilitiesControl::new(
            swarm_control.clone(),
            shared_config.clone(),
            runtime_control.clone(),
            state.incoming_allowed_peers().clone(),
        );
        node_capabilities_control.start()?;

        let tcp_tunneling_control = TcpTunnelingControl::new(swarm_control.clone());
        tcp_tunneling_control
            .init_from_config(&config.tcp_tunneling)
            .await;

        let service_control_protocol_control = ServiceControlProtocolControl::new(
            swarm_control.clone(),
            shared_config.clone(),
            fungi_home,
            runtime_control.clone(),
            tcp_tunneling_control.clone(),
            state.incoming_allowed_peers().clone(),
        );
        service_control_protocol_control.start()?;

        let proxy_ftp_task = if config.file_transfer.proxy_ftp.enabled {
            Some(tokio::spawn(crate::controls::start_ftp_proxy_service(
                config.file_transfer.proxy_ftp.host,
                config.file_transfer.proxy_ftp.port,
                ftc_control.clone(),
            )))
        } else {
            None
        };

        let proxy_webdav_task = if config.file_transfer.proxy_webdav.enabled {
            Some(tokio::spawn(crate::controls::start_webdav_proxy_service(
                config.file_transfer.proxy_webdav.host,
                config.file_transfer.proxy_webdav.port,
                ftc_control.clone(),
            )))
        } else {
            None
        };

        let task_handles = TaskHandles {
            swarm_task,
            proxy_ftp_task: Arc::new(Mutex::new(proxy_ftp_task)),
            proxy_webdav_task: Arc::new(Mutex::new(proxy_webdav_task)),
        };
        let daemon = Self {
            config: shared_config,
            address_book_config: Arc::new(Mutex::new(address_book_config)),
            args,
            swarm_control,
            mdns_control,
            fts_control,
            ftc_control,
            docker_control,
            tcp_tunneling_control,
            runtime_control,
            service_discovery_control,
            node_capabilities_control,
            service_control_protocol_control,
            task_handles,
        };

        daemon.restore_service_endpoint_listeners().await?;

        Ok(daemon)
    }

    pub async fn wait_all(self) {
        tokio::select! {
            _ = self.task_handles.swarm_task => {
                println!("Swarm task is closed");
            },
            // _ = self.task_handles.daemon_rpc_task => {
            //     println!("Daemon RPC task is closed");
            // },
        }
    }

    async fn init_fts(config: FTSConfig, fts_control: &FileTransferServiceControl) {
        if config.enabled
            && let Err(e) = fts_control.add_service(config).await
        {
            log::warn!("Failed to add file transfer service: {e}");
        }
    }

    async fn restore_service_endpoint_listeners(&self) -> Result<()> {
        let mut listening_rules = self.tcp_tunneling_control.get_listening_rules();

        for service in self.runtime_control.list_services().await? {
            if !service.status.running {
                continue;
            }

            for endpoint in service.exposed_endpoints {
                let already_present = listening_rules.iter().any(|(_, rule)| {
                    rule.host == "127.0.0.1"
                        && rule.port == endpoint.host_port
                        && rule.protocol.as_deref() == Some(endpoint.protocol.as_str())
                });
                if already_present {
                    continue;
                }

                let rule = fungi_config::tcp_tunneling::ListeningRule {
                    host: "127.0.0.1".to_string(),
                    port: endpoint.host_port,
                    protocol: Some(endpoint.protocol),
                };

                match self
                    .tcp_tunneling_control
                    .add_listening_rule(rule.clone())
                    .await
                {
                    Ok(rule_id) => listening_rules.push((rule_id, rule)),
                    Err(error) => {
                        log::warn!(
                            "Failed to restore service endpoint listener on 127.0.0.1:{}: {}",
                            endpoint.host_port,
                            error
                        );
                    }
                }
            }
        }

        Ok(())
    }

    fn init_ftc(clients: Vec<FTCConfig>, ftc_control: FileTransferClientsControl) {
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(2)).await;
            for mut client in clients {
                if !client.enabled {
                    continue;
                }
                if client.name.is_none()
                    && let Ok(remote_host_name) =
                        ftc_control.connect_and_get_host_name(client.peer_id).await
                {
                    client.name = remote_host_name
                }
                ftc_control.add_client(client);
            }
        });
    }

    pub(crate) fn update_ftp_proxy_task(
        &self,
        enabled: bool,
        host: IpAddr,
        port: u16,
    ) -> Result<()> {
        if port == 0 {
            return Err(anyhow::anyhow!("Port must be greater than 0"));
        }
        if let Some(old_task) = self.task_handles.proxy_ftp_task.lock().take()
            && !old_task.is_finished()
        {
            old_task.abort();
        }
        if enabled {
            let task = tokio::spawn(crate::controls::start_ftp_proxy_service(
                host,
                port,
                self.ftc_control.clone(),
            ));
            self.task_handles.proxy_ftp_task.lock().replace(task);
        }
        Ok(())
    }

    pub(crate) fn update_webdav_proxy_task(
        &self,
        enabled: bool,
        host: IpAddr,
        port: u16,
    ) -> Result<()> {
        if port == 0 {
            return Err(anyhow::anyhow!("Port must be greater than 0"));
        }
        if let Some(old_task) = self.task_handles.proxy_webdav_task.lock().take()
            && !old_task.is_finished()
        {
            old_task.abort();
        }
        if enabled {
            let task = tokio::spawn(crate::controls::start_webdav_proxy_service(
                host,
                port,
                self.ftc_control.clone(),
            ));
            self.task_handles.proxy_webdav_task.lock().replace(task);
        }
        Ok(())
    }

    pub(crate) async fn add_tcp_forwarding_rule_internal(
        &self,
        rule: fungi_config::tcp_tunneling::ForwardingRule,
    ) -> Result<String> {
        let rule_id = self
            .tcp_tunneling_control
            .add_forwarding_rule(rule.clone())
            .await?;

        // Update config file
        self.update_config_with_forwarding_rule(rule, true)?;

        Ok(rule_id)
    }

    pub(crate) fn remove_tcp_forwarding_rule_internal(&self, rule_id: &str) -> Result<()> {
        // Get the rule before removing it
        let rules = self.tcp_tunneling_control.get_forwarding_rules();
        let rule = rules
            .iter()
            .find(|(id, _)| id == rule_id)
            .map(|(_, rule)| rule.clone())
            .ok_or_else(|| anyhow::anyhow!("Forwarding rule not found: {}", rule_id))?;

        self.tcp_tunneling_control.remove_forwarding_rule(rule_id)?;

        // Update config file
        self.update_config_with_forwarding_rule(rule, false)?;

        Ok(())
    }

    pub(crate) async fn add_tcp_listening_rule_internal(
        &self,
        rule: fungi_config::tcp_tunneling::ListeningRule,
    ) -> Result<String> {
        let rule_id = self
            .tcp_tunneling_control
            .add_listening_rule(rule.clone())
            .await?;

        // Update config file
        self.update_config_with_listening_rule(rule, true)?;

        Ok(rule_id)
    }

    pub(crate) fn remove_tcp_listening_rule_internal(&self, rule_id: &str) -> Result<()> {
        // Get the rule before removing it
        let rules = self.tcp_tunneling_control.get_listening_rules();
        let rule = rules
            .iter()
            .find(|(id, _)| id == rule_id)
            .map(|(_, rule)| rule.clone())
            .ok_or_else(|| anyhow::anyhow!("Listening rule not found: {}", rule_id))?;

        self.tcp_tunneling_control.remove_listening_rule(rule_id)?;

        // Update config file
        self.update_config_with_listening_rule(rule, false)?;

        Ok(())
    }

    fn update_config_with_forwarding_rule(
        &self,
        rule: fungi_config::tcp_tunneling::ForwardingRule,
        add: bool,
    ) -> Result<()> {
        let current_config = self.config.lock().clone();
        let updated_config = if add {
            current_config.add_tcp_forwarding_rule(rule)?
        } else {
            current_config.remove_tcp_forwarding_rule(&rule)?
        };

        // Update the cached config
        *self.config.lock() = updated_config;
        Ok(())
    }

    fn update_config_with_listening_rule(
        &self,
        rule: fungi_config::tcp_tunneling::ListeningRule,
        add: bool,
    ) -> Result<()> {
        let current_config = self.config.lock().clone();
        let updated_config = if add {
            current_config.add_tcp_listening_rule(rule)?
        } else {
            current_config.remove_tcp_listening_rule(&rule)?
        };

        // Update the cached config
        *self.config.lock() = updated_config;
        Ok(())
    }
}

fn apply_listen(swarm: &mut TSwarm, config: &FungiConfig) {
    swarm
        .listen_on(
            Multiaddr::empty()
                .with(Protocol::from(Ipv4Addr::UNSPECIFIED))
                .with(Protocol::Tcp(config.network.listen_tcp_port)),
        )
        .unwrap();
    swarm
        .listen_on(
            Multiaddr::empty()
                .with(Protocol::from(Ipv6Addr::UNSPECIFIED))
                .with(Protocol::Tcp(config.network.listen_tcp_port)),
        )
        .unwrap();
    swarm
        .listen_on(
            Multiaddr::empty()
                .with(Protocol::from(Ipv6Addr::UNSPECIFIED))
                .with(Protocol::Udp(config.network.listen_udp_port))
                .with(Protocol::QuicV1),
        )
        .unwrap();
    swarm
        .listen_on(
            Multiaddr::empty()
                .with(Protocol::from(Ipv4Addr::UNSPECIFIED))
                .with(Protocol::Udp(config.network.listen_udp_port))
                .with(Protocol::QuicV1),
        )
        .unwrap();
}
