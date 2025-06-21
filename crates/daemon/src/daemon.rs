use crate::{
    DaemonArgs,
    controls::{FileTransferClientControl, FileTransferServiceControl},
    listeners::FungiDaemonRpcServer,
};
use anyhow::Result;
use fungi_config::{
    FungiConfig, FungiDir, file_transfer::FileTransferClient as FTCConfig,
    file_transfer::FileTransferService as FTSConfig,
};
use fungi_swarm::{FungiSwarm, State, SwarmControl, TSwarm};
use fungi_util::keypair::get_keypair_from_dir;
use tokio::task::JoinHandle;

struct TaskHandles {
    swarm_task: JoinHandle<()>,
    daemon_rpc_task: JoinHandle<()>,
    proxy_ftp_task: Option<JoinHandle<()>>,
    proxy_webdav_task: Option<JoinHandle<()>>,
}

pub struct FungiDaemon {
    config: FungiConfig,
    args: DaemonArgs,

    pub swarm_control: SwarmControl,
    pub fts_control: FileTransferServiceControl,

    task_handles: TaskHandles,
}

impl FungiDaemon {
    pub async fn start(args: DaemonArgs) -> Result<Self> {
        let fungi_dir = args.fungi_dir();
        println!("Fungi directory: {:?}", fungi_dir);

        let config = FungiConfig::apply_from_dir(&fungi_dir).unwrap();

        let state = State::new(
            config
                .libp2p
                .incoming_allowed_peers
                .clone()
                .into_iter()
                .collect(),
        );

        let keypair = get_keypair_from_dir(&fungi_dir).unwrap();
        let (swarm_control, swarm_task) = FungiSwarm::start_swarm(keypair, state, |swarm| {
            apply_listen(swarm, &config);
            #[cfg(feature = "tcp-tunneling")]
            apply_tcp_tunneling(swarm, &config);
        })
        .await?;

        let stream_control = swarm_control.stream_control().clone();

        let fts_control = FileTransferServiceControl::new(stream_control.clone());
        Self::init_fts(config.file_transfer.server.clone(), &fts_control);

        let ftc_control = FileTransferClientControl::new(swarm_control.clone());
        Self::init_ftc(config.file_transfer.client.clone(), &ftc_control);

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
            proxy_ftp_task,
            proxy_webdav_task,
        };
        Ok(Self {
            config,
            args,
            swarm_control,
            fts_control,
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

    fn init_fts(config: FTSConfig, fts_control: &FileTransferServiceControl) {
        if config.enabled {
            if let Err(e) = fts_control.add_service(config) {
                log::warn!("Failed to add file transfer service: {}", e);
            }
        }
    }

    fn init_ftc(clients: Vec<FTCConfig>, ftc_control: &FileTransferClientControl) {
        for client in clients {
            ftc_control.add_client(client);
        }
    }
}

fn apply_listen(swarm: &mut TSwarm, config: &FungiConfig) {
    swarm
        .listen_on(
            format!("/ip4/0.0.0.0/tcp/{}", config.libp2p.listen_tcp_port)
                .parse()
                .expect("address should be valid"),
        )
        .unwrap();
    swarm
        .listen_on(
            format!("/ip4/0.0.0.0/udp/{}/quic-v1", config.libp2p.listen_udp_port)
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
