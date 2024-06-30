use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use fungi_gateway::{SwarmState, TSwarm};
use fungi_util::tcp_tunneling;
use libp2p::StreamProtocol;

use crate::config::FungiConfig;

use super::listeners::{ContainerListener, ShellListener};

pub struct FungiDaemon {
    pub swarm_state: Arc<Mutex<SwarmState>>,
    config: FungiConfig,
    fungi_dir: PathBuf,
    shell_listener: ShellListener,
    container_listener: ContainerListener,
}

impl FungiDaemon {
    pub async fn new(fungi_dir: PathBuf, config: FungiConfig) -> Self {
        let swarm = SwarmState::new(&fungi_dir, |mut swarm| {
            apply_listen(&mut swarm, &config);
            #[cfg(feature = "tcp-tunneling")]
            apply_tcp_tunneling(&mut swarm, &config);
            swarm
        })
        .await
        .unwrap();

        let container_listener = ContainerListener::new();
        Self {
            swarm_state: Arc::new(Mutex::new(swarm)),
            config,
            fungi_dir,
            shell_listener: ShellListener::new(container_listener.clone()),
            container_listener,
        }
    }

    pub async fn start(&mut self) {
        self.swarm_state.lock().unwrap().start_swarm_task();
        self.shell_listener
            .start(format!("127.0.0.1:6010").parse().unwrap())
            .await.unwrap();
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
