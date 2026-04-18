use std::{
    convert::Infallible,
    future::{Ready, ready},
    sync::Arc,
};

use libp2p_core::{InboundUpgrade, OutboundUpgrade, UpgradeInfo};
use libp2p_identity::PeerId;
use libp2p_swarm::{ConnectionId, Stream, StreamProtocol};
use parking_lot::Mutex;
use thiserror::Error;

use crate::{
    control::IncomingStream,
    policy::{AuthorizationRejectReason, authorize_inbound},
    registry::Registry,
};

pub struct InboundStreamUpgrade {
    peer_id: PeerId,
    connection_id: ConnectionId,
    registry: Arc<Mutex<Registry>>,
    supported_protocols: Vec<StreamProtocol>,
}

impl InboundStreamUpgrade {
    pub(crate) fn new(
        peer_id: PeerId,
        connection_id: ConnectionId,
        registry: Arc<Mutex<Registry>>,
        supported_protocols: Vec<StreamProtocol>,
    ) -> Self {
        Self {
            peer_id,
            connection_id,
            registry,
            supported_protocols,
        }
    }
}

impl UpgradeInfo for InboundStreamUpgrade {
    type Info = StreamProtocol;
    type InfoIter = std::vec::IntoIter<StreamProtocol>;

    fn protocol_info(&self) -> Self::InfoIter {
        self.supported_protocols.clone().into_iter()
    }
}

impl InboundUpgrade<Stream> for InboundStreamUpgrade {
    type Output = IncomingStream;
    type Error = InboundUpgradeError;
    type Future = Ready<Result<Self::Output, Self::Error>>;

    fn upgrade_inbound(self, socket: Stream, info: Self::Info) -> Self::Future {
        // Authorize here, before the stream is exposed to protocol code. This keeps the
        // security boundary centralized and avoids duplicated peer checks in every handler.
        let (global_allow_list, protocol_allow_list) = {
            let mut registry = Registry::lock(&self.registry);
            (
                registry.global_allow_policy(),
                registry.protocol_allow_list(&info),
            )
        };

        let Some(protocol_allow_list) = protocol_allow_list else {
            // The protocol list is snapshotted when the handler advertises supported protocols.
            // If the listener disappears before the negotiated stream reaches upgrade_inbound,
            // fail closed and report that the protocol is no longer registered.
            return ready(Err(InboundUpgradeError::UnsupportedProtocol {
                peer_id: self.peer_id,
                connection_id: self.connection_id,
                protocol: info,
            }));
        };

        match authorize_inbound(&global_allow_list, &protocol_allow_list, self.peer_id) {
            Ok(()) => ready(Ok(IncomingStream {
                peer_id: self.peer_id,
                connection_id: self.connection_id,
                protocol: info,
                stream: socket,
            })),
            Err(reason) => ready(Err(InboundUpgradeError::Unauthorized {
                peer_id: self.peer_id,
                connection_id: self.connection_id,
                protocol: info,
                reason,
            })),
        }
    }
}

pub struct OutboundStreamUpgrade {
    supported_protocols: Vec<StreamProtocol>,
}

impl OutboundStreamUpgrade {
    pub(crate) fn new(supported_protocols: Vec<StreamProtocol>) -> Self {
        Self {
            supported_protocols,
        }
    }
}

impl UpgradeInfo for OutboundStreamUpgrade {
    type Info = StreamProtocol;
    type InfoIter = std::vec::IntoIter<StreamProtocol>;

    fn protocol_info(&self) -> Self::InfoIter {
        self.supported_protocols.clone().into_iter()
    }
}

impl OutboundUpgrade<Stream> for OutboundStreamUpgrade {
    type Output = (Stream, StreamProtocol);
    type Error = Infallible;
    type Future = Ready<Result<Self::Output, Self::Error>>;

    fn upgrade_outbound(self, socket: Stream, info: Self::Info) -> Self::Future {
        ready(Ok((socket, info)))
    }
}

#[derive(Debug, Error)]
pub enum InboundUpgradeError {
    #[error(
        "inbound stream for protocol {protocol} on connection {connection_id} from {peer_id} was rejected because no active listener was registered"
    )]
    UnsupportedProtocol {
        peer_id: PeerId,
        connection_id: ConnectionId,
        protocol: StreamProtocol,
    },
    #[error(
        "inbound stream for protocol {protocol} on connection {connection_id} from {peer_id} was rejected by authorization: {reason:?}"
    )]
    Unauthorized {
        peer_id: PeerId,
        connection_id: ConnectionId,
        protocol: StreamProtocol,
        reason: AuthorizationRejectReason,
    },
}
