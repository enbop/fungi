use crate::{
    listeners::{FRALocalListener, FRAPeerListener, FungiDaemonRpcServer},
    DaemonArgs,
};
use anyhow::Result;
use fungi_config::{FungiConfig, FungiDir};
use fungi_swarm::{FungiSwarm, SwarmController, TSwarm};
use fungi_util::keypair::get_keypair_from_dir;
use std::path::PathBuf;
use tokio::{sync::OnceCell, task::JoinHandle};

static FUNGI_BIN_PATH: OnceCell<PathBuf> = OnceCell::const_new();

struct TaskHandles {
    swarm_task: JoinHandle<()>,
    fra_local_listener_task: JoinHandle<()>,
    fra_remote_listener_task: JoinHandle<()>,
    daemon_rpc_task: JoinHandle<()>,
}

pub struct FungiDaemon {
    config: FungiConfig,
    args: DaemonArgs,

    pub swarm_controller: SwarmController,

    task_handles: TaskHandles,
}

impl FungiDaemon {
    pub async fn start(args: DaemonArgs) -> Result<Self> {
        let fungi_dir = args.fungi_dir();
        println!("Fungi directory: {:?}", fungi_dir);

        FungiDaemon::init_fungi_bin_path(&args);

        let mut config = FungiConfig::apply_from_dir(&fungi_dir).unwrap();
        if let Some(allow_all_peers) = args.debug_allow_all_peers {
            config.set_fra_allow_all_peers(allow_all_peers);
        }

        let keypair = get_keypair_from_dir(&fungi_dir).unwrap();
        let (swarm_controller, swarm_task) = FungiSwarm::start_swarm(keypair, |swarm| {
            apply_listen(swarm, &config);
            #[cfg(feature = "tcp-tunneling")]
            apply_tcp_tunneling(swarm, &config);
        })
        .await?;

        let stream_control = swarm_controller.stream_control.clone();

        let fra_local_listener_task =
            FRALocalListener::start(args.clone(), stream_control.clone())?;
        let fra_remote_listener_task =
            FRAPeerListener::start(args.clone(), config.clone(), stream_control)?;

        let daemon_rpc_task = FungiDaemonRpcServer::start(args.clone(), swarm_controller.clone())?;

        let task_handles = TaskHandles {
            swarm_task,
            fra_local_listener_task,
            fra_remote_listener_task,
            daemon_rpc_task,
        };
        Ok(Self {
            config,
            args,
            swarm_controller,
            task_handles,
        })
    }

    pub async fn wait_all(self) {
        tokio::select! {
            _ = self.task_handles.swarm_task => {
                println!("Swarm task is closed");
            },
            _ = self.task_handles.fra_local_listener_task => {
                println!("FRA local listener task is closed");
            },
            _ = self.task_handles.fra_remote_listener_task => {
                println!("FRA remote listener task is closed");
            },
            _ = self.task_handles.daemon_rpc_task => {
                println!("Daemon RPC task is closed");
            },
        }
    }

    #[allow(unused_variables)]
    fn init_fungi_bin_path(args: &DaemonArgs) {
        let fungi_bin_path = args.fungi_bin_path.clone().map(PathBuf::from);
        if let Some(fungi_bin_path) = fungi_bin_path {
            FUNGI_BIN_PATH.set(fungi_bin_path).unwrap();
            return;
        }

        #[cfg(feature = "daemon")]
        let all_in_one_bin = true;
        #[cfg(not(feature = "daemon"))]
        let all_in_one_bin = false;

        let self_bin = std::env::current_exe().unwrap();
        let fungi_bin_path = if all_in_one_bin {
            self_bin
        } else {
            self_bin.parent().unwrap().join("fungi")
        };
        FUNGI_BIN_PATH.set(fungi_bin_path).unwrap();
    }

    pub fn get_fungi_bin_path_unchecked() -> PathBuf {
        FUNGI_BIN_PATH.get().unwrap().clone()
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

            swarm
                .behaviour_mut()
                .address_book
                .set_addresses(&target_peer, rule.remote.multiaddrs.clone());

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
