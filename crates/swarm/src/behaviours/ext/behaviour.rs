use std::{
    convert::Infallible,
    fmt,
    ops::Deref,
    task::{Context, Poll},
};

use libp2p::{
    Multiaddr, PeerId,
    core::{Endpoint, transport::PortUse},
    swarm::{
        ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour, THandler, THandlerInEvent,
        THandlerOutEvent, ToSwarm, dummy,
    },
};

use crate::State;

/// [`libp2p::allow_block_list`]
#[derive(Debug)]
pub struct NotAllowed {
    peer: PeerId,
}

impl fmt::Display for NotAllowed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "peer {} is not in the allow list", self.peer)
    }
}

impl std::error::Error for NotAllowed {}

pub struct Behaviour {
    state: State,
}

impl Deref for Behaviour {
    type Target = State;

    fn deref(&self) -> &Self::Target {
        &self.state
    }
}

impl Behaviour {
    pub fn new(state: State) -> Self {
        Self { state }
    }
}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler = dummy::ConnectionHandler;
    type ToSwarm = Infallible;

    fn handle_established_inbound_connection(
        &mut self,
        _: ConnectionId,
        peer: PeerId,
        _: &Multiaddr,
        _: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        if !self.incoming_allowed_peers().read().contains(&peer) {
            return Err(ConnectionDenied::new(NotAllowed { peer }));
        }

        Ok(dummy::ConnectionHandler)
    }

    fn handle_established_outbound_connection(
        &mut self,
        _: ConnectionId,
        _: PeerId,
        _: &Multiaddr,
        _: Endpoint,
        _: PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(dummy::ConnectionHandler)
    }

    fn on_connection_handler_event(
        &mut self,
        _: PeerId,
        _: ConnectionId,
        _event: THandlerOutEvent<Self>,
    ) {
    }

    fn poll(&mut self, _: &mut Context<'_>) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        Poll::Pending
    }

    fn on_swarm_event(&mut self, _event: FromSwarm) {}
}
