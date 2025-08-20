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

const DEFAULT_CONFIG_DIR: &'static str = ".fungi-relay-server";
const DEFAULT_LISTEN_PORT: u16 = 30001;

#[derive(Debug, Clone, Parser)]
pub struct RelayArgs {
    #[clap(
        short,
        long,
        help = "Path to the Fungi relay server config directory, defaults to ~/.fungi-relay-server"
    )]
    pub fungi_dir: Option<String>,

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
}

#[derive(NetworkBehaviour)]
struct Behaviour {
    relay: relay::Behaviour,
    ping: ping::Behaviour,
    identify: identify::Behaviour,
    stream: libp2p_stream::Behaviour,
}

pub async fn run(args: RelayArgs) -> Result<()> {
    let public_ip = args.public_ip;
    let tcp_listen_port = args.tcp_listen_port;
    let udp_listen_port = args.udp_listen_port;

    let keypair = get_or_init_keypair()?;

    let mut swarm = libp2p::SwarmBuilder::with_existing_identity(keypair)
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_quic()
        .with_behaviour(|key| Behaviour {
            relay: relay::Behaviour::new(key.public().to_peer_id(), Default::default()),
            ping: ping::Behaviour::new(ping::Config::new()),
            identify: identify::Behaviour::new(identify::Config::new(
                "/fungi-relay/0.1.0".to_string(),
                key.public(),
            )),
            stream: libp2p_stream::Behaviour::default(),
        })?
        .build();

    println!("Local peer id: {:?}", swarm.local_peer_id());
    listen_all_interfaces(&mut swarm, tcp_listen_port, udp_listen_port)?;
    add_external_address(&mut swarm, public_ip, tcp_listen_port, udp_listen_port)?;
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
    public_ip: IpAddr,
    tcp_listen_port: u16,
    udp_listen_port: u16,
) -> Result<()> {
    swarm.add_external_address(
        Multiaddr::empty()
            .with(Protocol::from(public_ip))
            .with(Protocol::Tcp(tcp_listen_port)),
    );
    swarm.add_external_address(
        Multiaddr::empty()
            .with(Protocol::from(public_ip))
            .with(Protocol::Udp(udp_listen_port))
            .with(Protocol::QuicV1),
    );
    Ok(())
}

fn listen_relay_handshake_protocol(mut stream_control: libp2p_stream::Control) {
    let mut listener = stream_control
        .accept(FUNGI_RELAY_HANDSHAKE_PROTOCOL)
        .unwrap();
    tokio::spawn(async move {
        loop {
            let Some((peer_id, mut stream)) = listener.next().await else {
                break;
            };
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
