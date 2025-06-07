pub mod address_book;

use libp2p::{dcutr, identify, identity::Keypair, mdns, ping, relay, swarm::NetworkBehaviour};

// default identify protocol name for libp2p
const IDENTIFY_PROTOCOL: &str = "/ipfs/id/1.0.0";

fn identify_user_agent() -> String {
    format!("fungi/{}", env!("CARGO_PKG_VERSION"))
}

#[derive(NetworkBehaviour)]
pub struct FungiBehaviours {
    ping: ping::Behaviour,
    pub stream: libp2p_stream::Behaviour,
    mdns: mdns::tokio::Behaviour,
    identify: identify::Behaviour,
    relay: relay::client::Behaviour,
    dcutr: dcutr::Behaviour,

    pub address_book: address_book::Behaviour,
}

impl FungiBehaviours {
    pub fn new(
        keypair: &Keypair,
        relay: relay::client::Behaviour,
        mdns: mdns::tokio::Behaviour,
    ) -> Self {
        let peer_id = keypair.public().to_peer_id();

        // Create a identify behaviour.
        let user_agent = identify_user_agent();
        let proto_version = IDENTIFY_PROTOCOL.to_string();
        let identify = identify::Behaviour::new(
            identify::Config::new(proto_version, keypair.public()).with_agent_version(user_agent),
        );

        Self {
            ping: ping::Behaviour::default(),
            stream: libp2p_stream::Behaviour::default(),
            mdns,
            identify,
            relay,
            dcutr: dcutr::Behaviour::new(peer_id),
            address_book: Default::default(),
        }
    }
}
