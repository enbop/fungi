use anyhow::Result;
use clap::Parser;
use fungi_util::protocols::FUNGI_RELAY_HANDSHAKE_PROTOCOL;
use libp2p::{
    Swarm,
    core::{Multiaddr, multiaddr::Protocol},
    futures::{AsyncWriteExt, StreamExt},
    identify,
    identity::Keypair,
    noise, ping, relay,
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux,
};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::Duration;

const DEFAULT_CONFIG_DIR: &str = ".fungi-relay-server";
const DEFAULT_LISTEN_PORT: u16 = 30001;
const DEFAULT_MAX_CIRCUIT_DURATION_SECS: u64 = 24 * 60 * 60;
const DEFAULT_MAX_CIRCUIT_BYTES: u64 = u64::MAX;

#[derive(Debug, Clone, Parser)]
pub struct RelayArgs {
    #[clap(short, long, help = "Public IP address of this device")]
    pub public_ip: IpAddr,

    #[clap(
        short,
        long,
        help = "Tcp listen port for the relay server, defaults to 30001",
        default_value_t = DEFAULT_LISTEN_PORT
    )]
    pub tcp_listen_port: u16,

    #[clap(
        short,
        long,
        help = "Udp listen port for the relay server, defaults to 30001",
        default_value_t = DEFAULT_LISTEN_PORT
    )]
    pub udp_listen_port: u16,

    #[clap(
        long,
        help = "Maximum lifetime of a relayed circuit in seconds",
        default_value_t = DEFAULT_MAX_CIRCUIT_DURATION_SECS
    )]
    pub max_circuit_duration_secs: u64,

    #[clap(
        long,
        help = "Maximum total bytes forwarded on a relayed circuit",
        default_value_t = DEFAULT_MAX_CIRCUIT_BYTES
    )]
    pub max_circuit_bytes: u64,
}

#[derive(NetworkBehaviour)]
struct Behaviour {
    relay: relay::Behaviour,
    ping: ping::Behaviour,
    identify: identify::Behaviour,
    stream: fungi_stream::Behaviour,
}

pub async fn run(args: RelayArgs) -> Result<()> {
    let public_ip = args.public_ip;
    let tcp_listen_port = args.tcp_listen_port;
    let udp_listen_port = args.udp_listen_port;
    let max_circuit_duration = Duration::from_secs(args.max_circuit_duration_secs);
    let max_circuit_bytes = args.max_circuit_bytes;

    let keypair = get_or_init_keypair()?;
    let relay_config = relay::Config {
        max_circuit_duration,
        max_circuit_bytes,
        ..Default::default()
    };

    let mut swarm = libp2p::SwarmBuilder::with_existing_identity(keypair)
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_quic()
        .with_behaviour(|key| Behaviour {
            relay: relay::Behaviour::new(key.public().to_peer_id(), relay_config),
            ping: ping::Behaviour::new(ping::Config::new()),
            identify: identify::Behaviour::new(identify::Config::new(
                "/fungi-relay/0.1.0".to_string(),
                key.public(),
            )),
            stream: fungi_stream::Behaviour::new_allow_all(),
        })?
        .build();

    let peer_id = *swarm.local_peer_id();
    println!("Local peer id: {}", swarm.local_peer_id());
    println!(
        "Relay limits: max_circuit_duration={:?}, max_circuit_bytes={}",
        max_circuit_duration, max_circuit_bytes
    );

    listen_all_interfaces(&mut swarm, tcp_listen_port, udp_listen_port)?;

    let tcp_listen_addr = Multiaddr::empty()
        .with(Protocol::from(public_ip))
        .with(Protocol::Tcp(tcp_listen_port));
    let udp_listen_addr = Multiaddr::empty()
        .with(Protocol::from(public_ip))
        .with(Protocol::Udp(udp_listen_port))
        .with(Protocol::QuicV1);

    add_external_address(&mut swarm, tcp_listen_addr.clone(), udp_listen_addr.clone())?;

    let tcp_listen_addr = tcp_listen_addr.with_p2p(peer_id).expect("with_p2p failed");
    let udp_listen_addr = udp_listen_addr.with_p2p(peer_id).expect("with_p2p failed");
    println!("Added external addresses: ");
    println!("{tcp_listen_addr}");
    println!("{udp_listen_addr}");

    listen_relay_handshake_protocol(swarm.behaviour().stream.new_control());

    loop {
        match swarm.next().await.expect("Infinite Stream.") {
            SwarmEvent::Behaviour(event) => {
                log::info!("{event:?}")
            }
            SwarmEvent::NewListenAddr { address, .. } => {
                log::info!("Listening on {address:?}");
            }
            _ => {}
        }
    }
}

fn listen_all_interfaces(
    swarm: &mut Swarm<Behaviour>,
    tcp_listen_port: u16,
    udp_listen_port: u16,
) -> Result<()> {
    // Listen on all interfaces
    swarm.listen_on(
        Multiaddr::empty()
            .with(Protocol::from(Ipv4Addr::UNSPECIFIED))
            .with(Protocol::Tcp(tcp_listen_port)),
    )?;
    swarm.listen_on(
        Multiaddr::empty()
            .with(Protocol::from(Ipv6Addr::UNSPECIFIED))
            .with(Protocol::Tcp(tcp_listen_port)),
    )?;
    swarm.listen_on(
        Multiaddr::empty()
            .with(Protocol::from(Ipv6Addr::UNSPECIFIED))
            .with(Protocol::Udp(udp_listen_port))
            .with(Protocol::QuicV1),
    )?;
    swarm.listen_on(
        Multiaddr::empty()
            .with(Protocol::from(Ipv4Addr::UNSPECIFIED))
            .with(Protocol::Udp(udp_listen_port))
            .with(Protocol::QuicV1),
    )?;
    Ok(())
}

fn add_external_address(
    swarm: &mut Swarm<Behaviour>,
    tcp_listen_addr: Multiaddr,
    udp_listen_addr: Multiaddr,
) -> Result<()> {
    swarm.add_external_address(tcp_listen_addr);
    swarm.add_external_address(udp_listen_addr);
    Ok(())
}

fn listen_relay_handshake_protocol(mut stream_control: fungi_stream::Control) {
    let mut listener = stream_control
        .listen(FUNGI_RELAY_HANDSHAKE_PROTOCOL)
        .unwrap();
    tokio::spawn(async move {
        loop {
            let Some(incoming_stream) = listener.next().await else {
                break;
            };
            let peer_id = incoming_stream.peer_id;
            let mut stream = incoming_stream.stream;
            log::info!("Accepted stream: {:?}", peer_id);
            // TODO: fungi relay handshake logic
            stream.write_all(b"ok").await.ok();
            stream.flush().await.ok();
            stream.close().await.ok();
        }
    });
}

fn get_or_init_keypair() -> Result<Keypair> {
    let config_dir = home::home_dir()
        .ok_or(anyhow::Error::msg("Failed to get home directory"))?
        .join(DEFAULT_CONFIG_DIR);
    let keypair = match fungi_util::keypair::get_keypair_from_dir(&config_dir) {
        Ok(keypair) => keypair,
        Err(_) => {
            println!("Initializing config dir...");
            std::fs::create_dir(&config_dir)?;
            fungi_util::keypair::init_keypair(&config_dir)?
        }
    };
    Ok(keypair)
}
