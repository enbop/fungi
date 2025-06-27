use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use crate::{
    DaemonArgs,
    controls::{FileTransferClientsControl, FileTransferServiceControl},
    listeners::FungiDaemonRpcServer,
};
use anyhow::Result;
use fungi_config::{
    FungiConfig, FungiDir, file_transfer::FileTransferClient as FTCConfig,
    file_transfer::FileTransferService as FTSConfig,
};
use fungi_swarm::{FungiSwarm, State, SwarmControl, TSwarm};
use fungi_util::keypair::get_keypair_from_dir;
use libp2p::PeerId;
use parking_lot::Mutex;
use tokio::task::JoinHandle;

struct TaskHandles {
    swarm_task: JoinHandle<()>,
    daemon_rpc_task: JoinHandle<()>,
    proxy_ftp_task: Arc<Mutex<Option<JoinHandle<()>>>>,
    proxy_webdav_task: Arc<Mutex<Option<JoinHandle<()>>>>,
}

pub struct FungiDaemon {
    config: Arc<Mutex<FungiConfig>>,
    args: DaemonArgs,

    swarm_control: SwarmControl,
    fts_control: FileTransferServiceControl,
    ftc_control: FileTransferClientsControl,

    task_handles: TaskHandles,
}

impl FungiDaemon {
    pub fn config(&self) -> Arc<Mutex<FungiConfig>> {
        self.config.clone()
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

    pub async fn start(args: DaemonArgs) -> Result<Self> {
        let fungi_dir = args.fungi_dir();
        println!("Fungi directory: {:?}", fungi_dir);

        let config = FungiConfig::apply_from_dir(&fungi_dir).unwrap();

        let state = State::new(
            config
                .network
                .incoming_allowed_peers
                .clone()
                .into_iter()
                .collect(),
        );

        let keypair = get_keypair_from_dir(&fungi_dir).unwrap();
        let (swarm_control, swarm_task) =
            FungiSwarm::start_swarm(keypair, state.clone(), |swarm| {
                apply_listen(swarm, &config);
                #[cfg(feature = "tcp-tunneling")]
                apply_tcp_tunneling(swarm, &config);
            })
            .await?;

        let stream_control = swarm_control.stream_control().clone();

        let fts_control = FileTransferServiceControl::new(
            stream_control.clone(),
            state.incoming_allowed_peers().clone(),
        );
        Self::init_fts(config.file_transfer.server.clone(), &fts_control).await;

        let ftc_control = FileTransferClientsControl::new(swarm_control.clone());
        Self::init_ftc(config.file_transfer.client.clone(), ftc_control.clone());

        let proxy_ftp_task = if config.file_transfer.proxy_ftp.enabled {
            Some(tokio::spawn(crate::controls::start_ftp_proxy_service(
                config.file_transfer.proxy_ftp.host.clone(),
                config.file_transfer.proxy_ftp.port,
                ftc_control.clone(),
            )))
        } else {
            None
        };

        let proxy_webdav_task = if config.file_transfer.proxy_webdav.enabled {
            Some(tokio::spawn(crate::controls::start_webdav_proxy_service(
                config.file_transfer.proxy_webdav.host.clone(),
                config.file_transfer.proxy_webdav.port,
                ftc_control.clone(),
            )))
        } else {
            None
        };

        let daemon_rpc_task = FungiDaemonRpcServer::start(args.clone(), swarm_control.clone())?;

        let task_handles = TaskHandles {
            swarm_task,
            daemon_rpc_task,
            proxy_ftp_task: Arc::new(Mutex::new(proxy_ftp_task)),
            proxy_webdav_task: Arc::new(Mutex::new(proxy_webdav_task)),
        };
        Ok(Self {
            config: Arc::new(Mutex::new(config)),
            args,
            swarm_control,
            fts_control,
            ftc_control,
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
                log::warn!("Failed to add file transfer service: {}", e);
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
                    if let Ok(remote_host_name) = ftc_control
                        .connect_and_get_host_name(client.peer_id.clone())
                        .await
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
}

fn apply_listen(swarm: &mut TSwarm, config: &FungiConfig) {
    swarm
        .listen_on(
            format!("/ip4/0.0.0.0/tcp/{}", config.network.listen_tcp_port)
                .parse()
                .expect("address should be valid"),
        )
        .unwrap();
    swarm
        .listen_on(
            format!(
                "/ip4/0.0.0.0/udp/{}/quic-v1",
                config.network.listen_udp_port
            )
            .parse()
            .expect("address should be valid"),
        )
        .unwrap();
}

#[cfg(feature = "tcp-tunneling")]
fn apply_tcp_tunneling(swarm: &mut TSwarm, config: &FungiConfig) {
    if config.tcp_tunneling.forwarding.enabled {
        for rule in config.tcp_tunneling.forwarding.rules.iter() {
            let Ok(target_peer) = rule.remote.peer_id.parse() else {
                continue;
            };

            let target_protocol =
                libp2p::StreamProtocol::try_from_owned(rule.remote.protocol.clone()).unwrap(); // TODO unwrap
            let stream_control = swarm.behaviour().stream.new_control();
            println!(
                "Forwarding local port {} to {}/{}",
                rule.local_socket.port, target_peer, target_protocol
            );
            tokio::spawn(fungi_util::tcp_tunneling::forward_port_to_peer(
                stream_control,
                (&rule.local_socket).try_into().unwrap(), // TOOD unwrap
                target_peer,
                target_protocol,
            ));
        }
    }

    if config.tcp_tunneling.listening.enabled {
        for rule in config.tcp_tunneling.listening.rules.iter() {
            let local_addr = (&rule.local_socket).try_into().unwrap(); // TODO unwrap
            let listening_protocol =
                libp2p::StreamProtocol::try_from_owned(rule.listening_protocol.clone()).unwrap(); // TODO unwrap
            let stream_control = swarm.behaviour().stream.new_control();
            println!("Listening on {} for {}", local_addr, listening_protocol);
            tokio::spawn(fungi_util::tcp_tunneling::listen_p2p_to_port(
                stream_control,
                listening_protocol,
                local_addr,
            ));
        }
    }
}
