use std::{net::{IpAddr, Ipv4Addr, Ipv6Addr}, sync::Arc, time::Duration};

use crate::{
    DaemonArgs,
    controls::{
        FileTransferClientsControl, FileTransferServiceControl, TcpTunnelingControl,
        mdns::MdnsControl,
    },
    listeners::FungiDaemonRpcServer,
};
use anyhow::Result;
use fungi_config::{
    FungiConfig, FungiDir,
    address_book::{AddressBookConfig, PeerInfo},
    file_transfer::{FileTransferClient as FTCConfig, FileTransferService as FTSConfig},
};
use fungi_swarm::{FungiSwarm, State, SwarmControl, TSwarm};
use fungi_util::keypair::get_keypair_from_dir;
use libp2p::{identity::Keypair, multiaddr::Protocol, Multiaddr};
use parking_lot::Mutex;
use tokio::task::JoinHandle;

#[allow(dead_code)]
struct TaskHandles {
    swarm_task: JoinHandle<()>,
    daemon_rpc_task: Arc<Mutex<Option<JoinHandle<()>>>>,
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
    tcp_tunneling_control: TcpTunnelingControl,

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

    pub fn tcp_tunneling_control(&self) -> &TcpTunnelingControl {
        &self.tcp_tunneling_control
    }

    pub fn mdns_control(&self) -> &MdnsControl {
        &self.mdns_control
    }

    pub async fn start(args: DaemonArgs) -> Result<Self> {
        let fungi_dir = args.fungi_dir();
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

        let (swarm_control, swarm_task) =
            FungiSwarm::start_swarm(keypair, state.clone(), relay_addrs, |swarm| {
                apply_listen(swarm, &config);
            })
            .await?;
        let mdns_control = MdnsControl::new();
        let peer_info = PeerInfo::this_device(swarm_control.local_peer_id(), config.get_hostname());
        mdns_control.start(peer_info)?;

        let stream_control = swarm_control.stream_control().clone();

        let fts_control = FileTransferServiceControl::new(
            stream_control.clone(),
            state.incoming_allowed_peers().clone(),
        );
        Self::init_fts(config.file_transfer.server.clone(), &fts_control).await;

        let ftc_control = FileTransferClientsControl::new(swarm_control.clone());
        Self::init_ftc(config.file_transfer.client.clone(), ftc_control.clone());

        let tcp_tunneling_control = TcpTunnelingControl::new(swarm_control.clone());
        tcp_tunneling_control
            .init_from_config(&config.tcp_tunneling)
            .await;

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

        let daemon_rpc_task = FungiDaemonRpcServer::start(args.clone(), swarm_control.clone())?;

        let task_handles = TaskHandles {
            swarm_task,
            daemon_rpc_task: Arc::new(Mutex::new(Some(daemon_rpc_task))),
            proxy_ftp_task: Arc::new(Mutex::new(proxy_ftp_task)),
            proxy_webdav_task: Arc::new(Mutex::new(proxy_webdav_task)),
        };
        Ok(Self {
            config: Arc::new(Mutex::new(config)),
            address_book_config: Arc::new(Mutex::new(address_book_config)),
            args,
            swarm_control,
            mdns_control,
            fts_control,
            ftc_control,
            tcp_tunneling_control,
            task_handles,
        })
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
        if config.enabled {
            if let Err(e) = fts_control.add_service(config).await {
                log::warn!("Failed to add file transfer service: {e}");
            }
        }
    }

    fn init_ftc(clients: Vec<FTCConfig>, ftc_control: FileTransferClientsControl) {
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(2)).await;
            for mut client in clients {
                if !client.enabled {
                    continue;
                }
                if client.name.is_none() {
                    if let Ok(remote_host_name) =
                        ftc_control.connect_and_get_host_name(client.peer_id).await
                    {
                        client.name = remote_host_name
                    }
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
        if let Some(old_task) = self.task_handles.proxy_ftp_task.lock().take() {
            if !old_task.is_finished() {
                old_task.abort();
            }
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
        if let Some(old_task) = self.task_handles.proxy_webdav_task.lock().take() {
            if !old_task.is_finished() {
                old_task.abort();
            }
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
    swarm.listen_on(
        Multiaddr::empty()
            .with(Protocol::from(Ipv4Addr::UNSPECIFIED))
            .with(Protocol::Tcp(config.network.listen_tcp_port)),
    ).unwrap();
    swarm.listen_on(
        Multiaddr::empty()
            .with(Protocol::from(Ipv6Addr::UNSPECIFIED))
            .with(Protocol::Tcp(config.network.listen_tcp_port)),
    ).unwrap();
    swarm.listen_on(
        Multiaddr::empty()
            .with(Protocol::from(Ipv6Addr::UNSPECIFIED))
            .with(Protocol::Udp(config.network.listen_udp_port))
            .with(Protocol::QuicV1),
    ).unwrap();
    swarm.listen_on(
        Multiaddr::empty()
            .with(Protocol::from(Ipv4Addr::UNSPECIFIED))
            .with(Protocol::Udp(config.network.listen_udp_port))
            .with(Protocol::QuicV1),
    ).unwrap();
}
