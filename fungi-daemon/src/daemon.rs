use crate::{
    listeners::{FRALocalListener, FRAPeerListener},
    DaemonArgs,
};
use fungi_config::{FungiConfig, FungiDir};
use fungi_swarm::{SwarmDaemon, TSwarm};
use std::path::PathBuf;
use tokio::sync::OnceCell;

static FUNGI_BIN_PATH: OnceCell<PathBuf> = OnceCell::const_new();

pub struct FungiDaemon {
    pub swarm_daemon: SwarmDaemon,
    config: FungiConfig,
    args: DaemonArgs,
    fra_local_listener: FRALocalListener,
    fra_remote_listener: FRAPeerListener,
}

impl FungiDaemon {
    pub async fn new(args: DaemonArgs) -> Self {
        let fungi_dir = args.fungi_dir();
        println!("Fungi directory: {:?}", fungi_dir);

        FungiDaemon::init_fungi_bin_path(&args);

        let mut config = FungiConfig::apply_from_dir(&fungi_dir).unwrap();
        if let Some(allow_all_peers) = args.debug_allow_all_peers {
            config.set_fra_allow_all_peers(allow_all_peers);
        }

        let swarm_daemon = SwarmDaemon::new(&fungi_dir, |swarm| {
            apply_listen(swarm, &config);
            #[cfg(feature = "tcp-tunneling")]
            apply_tcp_tunneling(swarm, &config);
        })
        .await
        .unwrap();

        let libp2p_stream_control = swarm_daemon.stream_control.clone();

        Self {
            swarm_daemon,
            fra_local_listener: FRALocalListener::new(args.clone(), libp2p_stream_control.clone()),
            fra_remote_listener: FRAPeerListener::new(
                args.clone(),
                config.clone(),
                libp2p_stream_control,
            ),
            args,
            config,
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

    pub async fn start(&mut self) {
        self.swarm_daemon.start_swarm_task();
        self.fra_local_listener.start().await.unwrap();
        self.fra_remote_listener.start().await.unwrap();
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
