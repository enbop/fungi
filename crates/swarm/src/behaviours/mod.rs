pub mod ext;

use std::ops::Deref;

use libp2p::{dcutr, identify, identity::Keypair, mdns, ping, relay, swarm::NetworkBehaviour};

use crate::State;

// default identify protocol name for libp2p
const IDENTIFY_PROTOCOL: &str = "/fungi/id/0.1.0";

fn identify_user_agent() -> String {
    format!("fungi/{}", env!("CARGO_PKG_VERSION"))
}

#[derive(NetworkBehaviour)]
pub struct FungiBehaviours {
    ping: ping::Behaviour,
    pub stream: libp2p_stream::Behaviour,
    pub mdns: mdns::tokio::Behaviour,
    identify: identify::Behaviour,
    relay: relay::client::Behaviour,
    dcutr: dcutr::Behaviour,

    pub fungi_ext: ext::Behaviour,
}

impl Deref for FungiBehaviours {
    type Target = ext::Behaviour;

    fn deref(&self) -> &Self::Target {
        &self.fungi_ext
    }
}

impl FungiBehaviours {
    pub fn new(
        keypair: &Keypair,
        relay: relay::client::Behaviour,
        mdns: mdns::tokio::Behaviour,
        state: State,
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
            fungi_ext: ext::Behaviour::new(state),
        }
    }
}
