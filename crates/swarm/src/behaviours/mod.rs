pub mod ext;
pub mod relay_refresh;

use std::collections::HashSet;

use std::ops::Deref;

use libp2p::{
    PeerId, dcutr, identify, identity::Keypair, mdns, ping as libp2p_ping, relay,
    swarm::NetworkBehaviour,
};

use crate::State;

// default identify protocol name for libp2p
const IDENTIFY_PROTOCOL: &str = "/fungi/id/0.1.0";

fn identify_user_agent() -> String {
    format!("fungi/{}", env!("CARGO_PKG_VERSION"))
}

#[derive(NetworkBehaviour)]
pub struct FungiBehaviours {
    pub stream: fungi_stream::Behaviour,
    relay_refresh: relay_refresh::Behaviour,
    pub mdns: mdns::tokio::Behaviour,
    ping: libp2p_ping::Behaviour,
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
        trusted_relay_peer_ids: Vec<PeerId>,
    ) -> Self {
        let peer_id = keypair.public().to_peer_id();
        let global_allow_list = state.incoming_allowed_peers();
        let trusted_relay_peer_ids = trusted_relay_peer_ids.into_iter().collect::<HashSet<_>>();

        // Create a identify behaviour.
        let user_agent = identify_user_agent();
        let proto_version = IDENTIFY_PROTOCOL.to_string();
        let identify = identify::Behaviour::new(
            identify::Config::new(proto_version, keypair.public()).with_agent_version(user_agent),
        );

        Self {
            stream: fungi_stream::Behaviour::new(global_allow_list),
            relay_refresh: relay_refresh::Behaviour::new_trusted_relays(trusted_relay_peer_ids),
            mdns,
            ping: libp2p_ping::Behaviour::new(libp2p_ping::Config::new()),
            identify,
            relay,
            dcutr: dcutr::Behaviour::new(peer_id),
            fungi_ext: ext::Behaviour::new(state),
        }
    }

    pub fn send_relay_refresh(&mut self, peer_id: &PeerId, announced_peer_id: PeerId) {
        self.relay_refresh.send(peer_id, announced_peer_id)
    }
}
