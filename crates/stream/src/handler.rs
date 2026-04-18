use std::{
    convert::Infallible,
    io,
    sync::Arc,
    task::{Context, Poll},
};

use futures::{
    StreamExt as _,
    channel::{mpsc, oneshot},
};
use libp2p_identity::PeerId;
use libp2p_swarm::{
    self as swarm, ConnectionHandler, ConnectionId, Stream, StreamProtocol,
    handler::{
        ConnectionEvent, DialUpgradeError, FullyNegotiatedInbound, FullyNegotiatedOutbound,
        ListenUpgradeError,
    },
};
use parking_lot::Mutex;

use crate::{
    OpenStreamError,
    registry::Registry,
    upgrade::{InboundStreamUpgrade, InboundUpgradeError, OutboundStreamUpgrade},
};

pub struct Handler {
    remote: PeerId,
    connection_id: ConnectionId,
    registry: Arc<Mutex<Registry>>,
    receiver: mpsc::Receiver<OpenRequest>,
    pending_upgrade: Option<(
        StreamProtocol,
        oneshot::Sender<Result<Stream, OpenStreamError>>,
    )>,
}

impl Handler {
    pub(crate) fn new(
        remote: PeerId,
        connection_id: ConnectionId,
        registry: Arc<Mutex<Registry>>,
        receiver: mpsc::Receiver<OpenRequest>,
    ) -> Self {
        Self {
            remote,
            connection_id,
            registry,
            receiver,
            pending_upgrade: None,
        }
    }
}

impl ConnectionHandler for Handler {
    type FromBehaviour = Infallible;
    type ToBehaviour = Infallible;
    type InboundProtocol = InboundStreamUpgrade;
    type OutboundProtocol = OutboundStreamUpgrade;
    type InboundOpenInfo = ();
    type OutboundOpenInfo = ();

    fn listen_protocol(&self) -> swarm::SubstreamProtocol<Self::InboundProtocol> {
        // Listener registrations are expected to be established during startup and stay stable
        // while the process is running. We still snapshot here so closed listener channels stop
        // being advertised, but fungi does not rely on hot-swapping protocol registrations.
        swarm::SubstreamProtocol::new(
            InboundStreamUpgrade::new(
                self.remote,
                self.connection_id,
                self.registry.clone(),
                Registry::lock(&self.registry).supported_inbound_protocols(),
            ),
            (),
        )
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<swarm::ConnectionHandlerEvent<Self::OutboundProtocol, (), Self::ToBehaviour>> {
        if self.pending_upgrade.is_some() {
            return Poll::Pending;
        }

        match self.receiver.poll_next_unpin(cx) {
            Poll::Ready(Some(request)) => {
                self.pending_upgrade = Some((request.protocol.clone(), request.response));
                return Poll::Ready(swarm::ConnectionHandlerEvent::OutboundSubstreamRequest {
                    protocol: swarm::SubstreamProtocol::new(
                        OutboundStreamUpgrade::new(vec![request.protocol]),
                        (),
                    ),
                });
            }
            Poll::Ready(None) => {}
            Poll::Pending => {}
        }

        Poll::Pending
    }

    fn on_behaviour_event(&mut self, event: Self::FromBehaviour) {
        libp2p_core::util::unreachable(event)
    }

    fn on_connection_event(
        &mut self,
        event: ConnectionEvent<Self::InboundProtocol, Self::OutboundProtocol>,
    ) {
        match event {
            ConnectionEvent::FullyNegotiatedInbound(FullyNegotiatedInbound {
                protocol: incoming,
                info: (),
            }) => {
                Registry::lock(&self.registry).on_inbound_stream(incoming);
            }
            ConnectionEvent::FullyNegotiatedOutbound(FullyNegotiatedOutbound {
                protocol: (stream, actual_protocol),
                info: (),
            }) => {
                let Some((expected_protocol, response)) = self.pending_upgrade.take() else {
                    debug_assert!(false, "Negotiated an outbound stream without a response");
                    return;
                };
                debug_assert_eq!(expected_protocol, actual_protocol);

                let _ = response.send(Ok(stream));
            }
            ConnectionEvent::DialUpgradeError(DialUpgradeError { error, info: () }) => {
                let Some((protocol, response)) = self.pending_upgrade.take() else {
                    debug_assert!(false, "Received DialUpgradeError without a response");
                    return;
                };

                let error = match error {
                    swarm::StreamUpgradeError::Timeout => {
                        OpenStreamError::Io(io::Error::from(io::ErrorKind::TimedOut))
                    }
                    swarm::StreamUpgradeError::Apply(value) => {
                        libp2p_core::util::unreachable(value)
                    }
                    swarm::StreamUpgradeError::NegotiationFailed => {
                        OpenStreamError::UnsupportedProtocol(protocol)
                    }
                    swarm::StreamUpgradeError::Io(error) => OpenStreamError::Io(error),
                };

                let _ = response.send(Err(error));
            }
            ConnectionEvent::ListenUpgradeError(ListenUpgradeError {
                error:
                    InboundUpgradeError::Unauthorized {
                        peer_id,
                        connection_id,
                        protocol,
                        reason,
                    },
                ..
            }) => {
                log::warn!(
                    "Rejected inbound stream for protocol {} from peer {} on connection {}: {:?}",
                    protocol,
                    peer_id,
                    connection_id,
                    reason
                );
            }
            ConnectionEvent::ListenUpgradeError(ListenUpgradeError {
                error:
                    InboundUpgradeError::UnsupportedProtocol {
                        peer_id,
                        connection_id,
                        protocol,
                    },
                ..
            }) => {
                log::debug!(
                    "Dropped inbound stream for unregistered protocol {} from peer {} on connection {}",
                    protocol,
                    peer_id,
                    connection_id,
                );
            }
            _ => {}
        }
    }
}

#[derive(Debug)]
pub(crate) struct OpenRequest {
    pub(crate) protocol: StreamProtocol,
    pub(crate) response: oneshot::Sender<Result<Stream, OpenStreamError>>,
}
