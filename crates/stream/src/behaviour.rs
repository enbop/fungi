use core::fmt;
use std::{
    sync::Arc,
    task::{Context, Poll},
};

use libp2p_core::{Endpoint, Multiaddr, transport::PortUse};
use libp2p_identity::PeerId;
use libp2p_swarm::{
    self as swarm, ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour, THandler,
    THandlerInEvent, THandlerOutEvent, ToSwarm,
};
use parking_lot::Mutex;
use swarm::behaviour::ConnectionClosed;

use crate::{
    Control,
    handler::Handler,
    policy::{GlobalAllowPolicy, SharedPeerAllowList},
    registry::Registry,
};

pub struct Behaviour {
    registry: Arc<Mutex<Registry>>,
}

impl Behaviour {
    pub fn new(global_allow_list: SharedPeerAllowList) -> Self {
        Self::new_with_global_policy(GlobalAllowPolicy::peer_set(global_allow_list))
    }

    pub fn new_allow_all() -> Self {
        Self::new_with_global_policy(GlobalAllowPolicy::allow_all())
    }

    fn new_with_global_policy(global_allow_policy: GlobalAllowPolicy) -> Self {
        Self {
            registry: Arc::new(Mutex::new(Registry::new(global_allow_policy))),
        }
    }

    pub fn new_control(&self) -> Control {
        Control::new(self.registry.clone())
    }
}

#[derive(Debug)]
pub struct AlreadyRegistered;

impl fmt::Display for AlreadyRegistered {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "The protocol is already registered")
    }
}

impl std::error::Error for AlreadyRegistered {}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler = Handler;
    type ToSwarm = ();

    fn handle_established_inbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        _: &Multiaddr,
        _: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        // Do not enforce peer authorization at connection establishment. Existing fungi
        // semantics allow broader connection graphs while stream access is enforced later,
        // at the protocol boundary, by fungi-stream itself.
        let receiver = Registry::lock(&self.registry).attach_connection(connection_id);
        Ok(Handler::new(
            peer,
            connection_id,
            self.registry.clone(),
            receiver,
        ))
    }

    fn handle_established_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        _: &Multiaddr,
        _: Endpoint,
        _: PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        let receiver = Registry::lock(&self.registry).attach_connection(connection_id);
        Ok(Handler::new(
            peer,
            connection_id,
            self.registry.clone(),
            receiver,
        ))
    }

    fn on_swarm_event(&mut self, event: FromSwarm) {
        if let FromSwarm::ConnectionClosed(ConnectionClosed { connection_id, .. }) = event {
            Registry::lock(&self.registry).on_connection_closed(connection_id);
        }
    }

    fn on_connection_handler_event(
        &mut self,
        _: PeerId,
        _: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        libp2p_core::util::unreachable(event)
    }

    fn poll(&mut self, _: &mut Context<'_>) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        Poll::Pending
    }
}
