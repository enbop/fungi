use std::{
    collections::{HashMap, hash_map::Entry},
    sync::Arc,
};

use futures::channel::mpsc;
use libp2p_swarm::{ConnectionId, StreamProtocol};
use parking_lot::{Mutex, MutexGuard};

use crate::{
    AlreadyRegistered,
    control::{IncomingStream, IncomingStreams},
    handler::OpenRequest,
    policy::{GlobalAllowPolicy, ProtocolAllowList},
};

pub(crate) struct Registry {
    global_allow_policy: GlobalAllowPolicy,
    listeners: HashMap<StreamProtocol, ListenerRegistration>,
    connections: HashMap<ConnectionId, ConnectionEntry>,
}

pub(crate) struct ListenerRegistration {
    // Authorization policy is stored next to the listener registration so inbound upgrade
    // can make a single, centralized allow/deny decision before exposing the stream.
    pub(crate) allow_list: ProtocolAllowList,
    pub(crate) sender: mpsc::Sender<IncomingStream>,
}

pub(crate) struct ConnectionEntry {
    pub(crate) outbound_sender: mpsc::Sender<OpenRequest>,
}

impl Registry {
    pub(crate) fn lock(registry: &Arc<Mutex<Registry>>) -> MutexGuard<'_, Registry> {
        registry.lock()
    }

    pub(crate) fn new(global_allow_policy: GlobalAllowPolicy) -> Self {
        Self {
            global_allow_policy,
            listeners: Default::default(),
            connections: Default::default(),
        }
    }

    pub(crate) fn register_listener(
        &mut self,
        protocol: StreamProtocol,
        allow_list: ProtocolAllowList,
    ) -> Result<IncomingStreams, AlreadyRegistered> {
        self.listeners
            .retain(|_, registration| !registration.sender.is_closed());

        if self.listeners.contains_key(&protocol) {
            return Err(AlreadyRegistered);
        }

        let (sender, receiver) = mpsc::channel(8);
        self.listeners
            .insert(protocol, ListenerRegistration { allow_list, sender });

        Ok(IncomingStreams::new(receiver))
    }

    pub(crate) fn unregister_listener(&mut self, protocol: &StreamProtocol) -> bool {
        self.listeners.remove(protocol).is_some()
    }

    pub(crate) fn supported_inbound_protocols(&mut self) -> Vec<StreamProtocol> {
        self.listeners
            .retain(|_, registration| !registration.sender.is_closed());
        self.listeners.keys().cloned().collect()
    }

    pub(crate) fn protocol_allow_list(
        &mut self,
        protocol: &StreamProtocol,
    ) -> Option<ProtocolAllowList> {
        self.listeners
            .retain(|_, registration| !registration.sender.is_closed());
        self.listeners
            .get(protocol)
            .map(|registration| registration.allow_list.clone())
    }

    pub(crate) fn global_allow_policy(&self) -> GlobalAllowPolicy {
        self.global_allow_policy.clone()
    }

    pub(crate) fn on_inbound_stream(&mut self, incoming: IncomingStream) {
        match self.listeners.entry(incoming.protocol.clone()) {
            Entry::Occupied(mut entry) => match entry.get_mut().sender.try_send(incoming) {
                Ok(()) => {}
                Err(error) if error.is_full() => {
                    // Backpressure is intentional: if the consumer is not polling fast enough,
                    // we prefer dropping the inbound stream over buffering unbounded state here.
                    log::debug!(
                        "Incoming listener channel is full, dropping inbound stream for protocol {}",
                        entry.key()
                    );
                }
                Err(error) if error.is_disconnected() => {
                    log::debug!(
                        "Incoming listener channel is disconnected, dropping inbound stream for protocol {}",
                        entry.key()
                    );
                    entry.remove();
                }
                Err(_) => unreachable!(),
            },
            Entry::Vacant(_) => {
                log::debug!(
                    "No active listener registration for protocol {}, dropping inbound stream",
                    incoming.protocol
                );
            }
        }
    }

    pub(crate) fn attach_connection(
        &mut self,
        connection_id: ConnectionId,
    ) -> mpsc::Receiver<OpenRequest> {
        let (outbound_sender, receiver) = mpsc::channel(0);
        self.connections
            .insert(connection_id, ConnectionEntry { outbound_sender });
        receiver
    }

    pub(crate) fn outbound_sender(
        &self,
        connection_id: ConnectionId,
    ) -> Option<mpsc::Sender<OpenRequest>> {
        self.connections
            .get(&connection_id)
            .map(|entry| entry.outbound_sender.clone())
    }

    pub(crate) fn on_connection_closed(&mut self, connection_id: ConnectionId) {
        self.connections.remove(&connection_id);
    }
}
