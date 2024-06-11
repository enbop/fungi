use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    task::Poll,
};

use libp2p::{
    core::Endpoint,
    swarm::{dummy, ConnectionDenied, ConnectionId, NetworkBehaviour},
    Multiaddr, PeerId,
};

#[derive(Default)]
pub struct Behaviour {
    address_book: Arc<RwLock<HashMap<PeerId, Vec<Multiaddr>>>>,
}

impl Behaviour {
    pub fn set_addresses(&mut self, peer_id: &PeerId, addrs: Vec<Multiaddr>) {
        self.address_book
            .write()
            .unwrap()
            .insert(*peer_id, addrs);
    }
}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler = dummy::ConnectionHandler;
    type ToSwarm = ();

    fn handle_pending_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        maybe_peer: Option<PeerId>,
        _addresses: &[Multiaddr],
        _effective_role: Endpoint,
    ) -> Result<Vec<Multiaddr>, ConnectionDenied> {
        let Some(peer) = maybe_peer else {
            return Ok(Vec::new());
        };
        Ok(self
            .address_book
            .write()
            .unwrap()
            .get(&peer)
            .unwrap_or(&Vec::new())
            .clone())
    }

    fn handle_established_inbound_connection(
        &mut self,
        _connection_id: libp2p::swarm::ConnectionId,
        _peer: PeerId,
        _local_addr: &Multiaddr,
        _remote_addr: &Multiaddr,
    ) -> Result<libp2p::swarm::THandler<Self>, libp2p::swarm::ConnectionDenied> {
        Ok(dummy::ConnectionHandler)
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: libp2p::swarm::ConnectionId,
        _peer: PeerId,
        _addr: &Multiaddr,
        _role_override: libp2p::core::Endpoint,
    ) -> Result<libp2p::swarm::THandler<Self>, libp2p::swarm::ConnectionDenied> {
        Ok(dummy::ConnectionHandler)
    }

    fn on_swarm_event(&mut self, _: libp2p::swarm::FromSwarm) {}

    fn on_connection_handler_event(
        &mut self,
        _peer_id: PeerId,
        _connection_id: libp2p::swarm::ConnectionId,
        ev: libp2p::swarm::THandlerOutEvent<Self>,
    ) {
        void::unreachable(ev)
    }

    fn poll(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<libp2p::swarm::ToSwarm<Self::ToSwarm, libp2p::swarm::THandlerInEvent<Self>>>
    {
        Poll::Pending
    }
}
