use super::listeners::{MushListener, WasiListener};
use crate::DaemonArgs;
use fungi_config::{FungiConfig, FungiDir};
use fungi_gateway::{SwarmDaemon, TSwarm};
use fungi_util::tcp_tunneling;
use libp2p::StreamProtocol;

pub struct FungiDaemon {
    pub swarm_daemon: SwarmDaemon,
    config: FungiConfig,
    args: DaemonArgs,
    mush_listener: MushListener,
    wasi_listener: WasiListener,
}

impl FungiDaemon {
    pub async fn new(args: DaemonArgs) -> Self {
        let fungi_dir = args.fungi_dir();
        println!("Fungi directory: {:?}", fungi_dir);

        let mut config = FungiConfig::apply_from_dir(&fungi_dir).unwrap();
        if let Some(allow_all_peers) = args.debug_allow_all_peers {
            config.set_mush_daemon_allow_all_peers(allow_all_peers);
        }

        let swarm_daemon = SwarmDaemon::new(&fungi_dir, |swarm| {
            apply_listen(swarm, &config);
            #[cfg(feature = "tcp-tunneling")]
            apply_tcp_tunneling(swarm, &config);
        })
        .await
        .unwrap();

        let libp2p_stream_control = swarm_daemon.stream_control.clone();

        let wasi_bin_path = args
            .wasi_bin_path
            .as_ref()
            .map(|path| path.parse().unwrap());
        let wasi_listener = WasiListener::new(fungi_dir.clone(), wasi_bin_path);
        Self {
            swarm_daemon,
            mush_listener: MushListener::new(
                args.clone(),
                config.clone(),
                wasi_listener.clone(),
                libp2p_stream_control,
            ),
            wasi_listener,
            args,
            config,
        }
    }

    pub async fn start(&mut self) {
        self.swarm_daemon.start_swarm_task();
        self.mush_listener.start().await.unwrap();
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
                StreamProtocol::try_from_owned(rule.remote.protocol.clone()).unwrap(); // TODO unwrap
            let stream_control = swarm.behaviour().stream.new_control();
            println!(
                "Forwarding local port {} to {}/{}",
                rule.local_socket.port, target_peer, target_protocol
            );
            tokio::spawn(tcp_tunneling::forward_port_to_peer(
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
                StreamProtocol::try_from_owned(rule.listening_protocol.clone()).unwrap(); // TODO unwrap
            let stream_control = swarm.behaviour().stream.new_control();
            println!("Listening on {} for {}", local_addr, listening_protocol);
            tokio::spawn(tcp_tunneling::listen_p2p_to_port(
                stream_control,
                listening_protocol,
                local_addr,
            ));
        }
    }
}
