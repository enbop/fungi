use super::listeners::{MushListener, WasiListener};
use crate::config::FungiConfig;
use fungi_gateway::{SwarmState, TSwarm};
use fungi_util::tcp_tunneling;
use libp2p::StreamProtocol;
use std::path::PathBuf;

pub struct FungiDaemon {
    pub swarm_state: SwarmState,
    config: FungiConfig,
    fungi_dir: PathBuf,
    mush_listener: MushListener,
    wasi_listener: WasiListener,
}

impl FungiDaemon {
    pub async fn new(fungi_dir: PathBuf, config: FungiConfig) -> Self {
        let swarm_state = SwarmState::new(&fungi_dir, |mut swarm| {
            apply_listen(&mut swarm, &config);
            #[cfg(feature = "tcp-tunneling")]
            apply_tcp_tunneling(&mut swarm, &config);
        })
        .await
        .unwrap();

        let wasi_listener = WasiListener::new();
        Self {
            swarm_state,
            config,
            fungi_dir,
            mush_listener: MushListener::new(wasi_listener.clone()),
            wasi_listener,
        }
    }

    pub async fn start(&mut self) {
        self.swarm_state.start_swarm_task();
        self.mush_listener
            .start(format!("127.0.0.1:6010").parse().unwrap())
            .await
            .unwrap();
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
