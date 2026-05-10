use core::fmt;
use std::{
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use futures::{
    SinkExt as _, StreamExt as _,
    channel::{mpsc, oneshot},
};
use libp2p_identity::PeerId;
use libp2p_swarm::{ConnectionId, Stream, StreamProtocol};
use parking_lot::Mutex;

use crate::{
    AlreadyRegistered, handler::OpenRequest, policy::ProtocolAllowList, registry::Registry,
};

#[derive(Clone)]
pub struct Control {
    registry: Arc<Mutex<Registry>>,
}

impl Control {
    pub(crate) fn new(registry: Arc<Mutex<Registry>>) -> Self {
        Self { registry }
    }

    // Default listener registration inherits the outer global allow list.
    pub fn listen(
        &mut self,
        protocol: StreamProtocol,
    ) -> Result<IncomingStreams, AlreadyRegistered> {
        self.listen_with_allow_list(protocol, ProtocolAllowList::inherit_global())
    }

    pub fn listen_with_allow_list(
        &mut self,
        protocol: StreamProtocol,
        allow_list: ProtocolAllowList,
    ) -> Result<IncomingStreams, AlreadyRegistered> {
        Registry::lock(&self.registry).register_listener(protocol, allow_list)
    }

    pub fn unlisten(&mut self, protocol: &StreamProtocol) -> bool {
        Registry::lock(&self.registry).unregister_listener(protocol)
    }

    pub async fn open_stream_by_id(
        &mut self,
        connection_id: ConnectionId,
        protocol: StreamProtocol,
    ) -> Result<Stream, OpenStreamError> {
        // Stream opening is connection-scoped on purpose. Connection selection lives in
        // fungi-swarm so this crate stays focused on stream negotiation and authorization.
        let mut outbound_sender = Registry::lock(&self.registry)
            .outbound_sender(connection_id)
            .ok_or(OpenStreamError::ConnectionNotFound(connection_id))?;

        let (response_sender, response_receiver) = oneshot::channel();

        outbound_sender
            .send(OpenRequest {
                protocol,
                response: response_sender,
            })
            .await
            .map_err(|_| OpenStreamError::ConnectionClosed)?;

        response_receiver
            .await
            .map_err(|_| OpenStreamError::ConnectionClosed)?
    }
}

#[derive(Debug)]
pub enum OpenStreamError {
    ConnectionNotFound(ConnectionId),
    ConnectionClosed,
    UnsupportedProtocol(StreamProtocol),
    Io(std::io::Error),
}

impl From<std::io::Error> for OpenStreamError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl fmt::Display for OpenStreamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OpenStreamError::ConnectionNotFound(connection_id) => {
                write!(
                    f,
                    "failed to open stream: connection {connection_id} not found"
                )
            }
            OpenStreamError::ConnectionClosed => {
                write!(f, "failed to open stream: connection is closed")
            }
            OpenStreamError::UnsupportedProtocol(protocol) => {
                write!(
                    f,
                    "failed to open stream: remote peer does not support {protocol}"
                )
            }
            OpenStreamError::Io(error) => {
                write!(f, "failed to open stream: io error: {error}")
            }
        }
    }
}

impl std::error::Error for OpenStreamError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

pub struct IncomingStream {
    pub peer_id: PeerId,
    pub connection_id: ConnectionId,
    pub protocol: StreamProtocol,
    pub stream: Stream,
}

#[must_use = "Streams do nothing unless polled."]
pub struct IncomingStreams {
    receiver: mpsc::Receiver<IncomingStream>,
}

impl IncomingStreams {
    pub(crate) fn new(receiver: mpsc::Receiver<IncomingStream>) -> Self {
        Self { receiver }
    }
}

impl futures::Stream for IncomingStreams {
    type Item = IncomingStream;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.receiver.poll_next_unpin(cx)
    }
}
