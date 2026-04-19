use std::{
    collections::{HashSet, VecDeque},
    future::Future,
    io, iter,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use fungi_util::protocols::FUNGI_RELAY_REFRESH_PROTOCOL;
use libp2p::{
    Multiaddr, PeerId, Stream,
    core::{Endpoint, InboundUpgrade, OutboundUpgrade, UpgradeInfo, transport::PortUse},
    futures::{AsyncReadExt, AsyncWriteExt},
    swarm::{
        ConnectionDenied, ConnectionHandler, ConnectionHandlerEvent, ConnectionId, FromSwarm,
        NetworkBehaviour, NotifyHandler, SubstreamProtocol, THandler, THandlerInEvent,
        THandlerOutEvent, ToSwarm,
        handler::{
            ConnectionEvent, DialUpgradeError, FullyNegotiatedInbound, FullyNegotiatedOutbound,
            ListenUpgradeError,
        },
    },
};
use serde::{Deserialize, Serialize};

const MAX_MESSAGE_BYTES: usize = 128;

#[derive(Debug)]
pub struct Event {
    pub peer: PeerId,
    pub announced_peer_id: PeerId,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Message {
    peer_id: PeerId,
}

#[derive(Clone)]
enum InboundPolicy {
    AllowAll,
    TrustedRelays(Arc<HashSet<PeerId>>),
}

impl InboundPolicy {
    fn allows(&self, peer: PeerId) -> bool {
        match self {
            Self::AllowAll => true,
            Self::TrustedRelays(trusted_relays) => trusted_relays.contains(&peer),
        }
    }
}

pub struct Behaviour {
    inbound_policy: InboundPolicy,
    queued_actions: VecDeque<ToSwarm<Event, Message>>,
}

impl Behaviour {
    pub fn new_trusted_relays<I>(trusted_relays: I) -> Self
    where
        I: IntoIterator<Item = PeerId>,
    {
        Self {
            inbound_policy: InboundPolicy::TrustedRelays(Arc::new(
                trusted_relays.into_iter().collect(),
            )),
            queued_actions: VecDeque::new(),
        }
    }

    pub fn new_allow_all() -> Self {
        Self {
            inbound_policy: InboundPolicy::AllowAll,
            queued_actions: VecDeque::new(),
        }
    }

    pub fn send(&mut self, peer_id: &PeerId, announced_peer_id: PeerId) {
        self.queued_actions.push_back(ToSwarm::NotifyHandler {
            peer_id: *peer_id,
            handler: NotifyHandler::Any,
            event: Message {
                peer_id: announced_peer_id,
            },
        });
    }
}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler = Handler;
    type ToSwarm = Event;

    fn handle_established_inbound_connection(
        &mut self,
        _: ConnectionId,
        peer: PeerId,
        _: &Multiaddr,
        _: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(Handler::new(peer, self.inbound_policy.clone()))
    }

    fn handle_established_outbound_connection(
        &mut self,
        _: ConnectionId,
        peer: PeerId,
        _: &Multiaddr,
        _: Endpoint,
        _: PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(Handler::new(peer, self.inbound_policy.clone()))
    }

    fn on_connection_handler_event(
        &mut self,
        peer: PeerId,
        _: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        let HandlerEvent::InboundAnnouncedPeer(announced_peer_id) = event;
        self.queued_actions.push_back(ToSwarm::GenerateEvent(Event {
            peer,
            announced_peer_id,
        }));
    }

    fn poll(&mut self, _: &mut Context<'_>) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        if let Some(action) = self.queued_actions.pop_front() {
            return Poll::Ready(action);
        }

        Poll::Pending
    }

    fn on_swarm_event(&mut self, _event: FromSwarm) {}
}

pub struct Handler {
    remote_peer: PeerId,
    inbound_policy: InboundPolicy,
    queued_events: VecDeque<ConnectionHandlerEvent<OutboundProtocol, (), HandlerEvent>>,
    pending_outbound: VecDeque<Message>,
}

impl Handler {
    fn new(remote_peer: PeerId, inbound_policy: InboundPolicy) -> Self {
        Self {
            remote_peer,
            inbound_policy,
            queued_events: VecDeque::new(),
            pending_outbound: VecDeque::new(),
        }
    }
}

impl ConnectionHandler for Handler {
    type FromBehaviour = Message;
    type ToBehaviour = HandlerEvent;
    type InboundProtocol = InboundProtocol;
    type OutboundProtocol = OutboundProtocol;
    type InboundOpenInfo = ();
    type OutboundOpenInfo = ();

    fn listen_protocol(&self) -> SubstreamProtocol<Self::InboundProtocol> {
        SubstreamProtocol::new(
            InboundProtocol::new(self.inbound_policy.allows(self.remote_peer)),
            (),
        )
    }

    fn on_behaviour_event(&mut self, event: Self::FromBehaviour) {
        self.pending_outbound.push_back(event);
    }

    fn poll(
        &mut self,
        _: &mut Context<'_>,
    ) -> Poll<ConnectionHandlerEvent<Self::OutboundProtocol, (), Self::ToBehaviour>> {
        if let Some(event) = self.queued_events.pop_front() {
            return Poll::Ready(event);
        }

        if let Some(request) = self.pending_outbound.pop_front() {
            return Poll::Ready(ConnectionHandlerEvent::OutboundSubstreamRequest {
                protocol: SubstreamProtocol::new(OutboundProtocol::new(request), ()),
            });
        }

        Poll::Pending
    }

    fn on_connection_event(
        &mut self,
        event: ConnectionEvent<Self::InboundProtocol, Self::OutboundProtocol>,
    ) {
        match event {
            ConnectionEvent::FullyNegotiatedInbound(FullyNegotiatedInbound {
                protocol: message,
                ..
            }) => {
                self.queued_events
                    .push_back(ConnectionHandlerEvent::NotifyBehaviour(
                        HandlerEvent::InboundAnnouncedPeer(message.peer_id),
                    ));
            }
            ConnectionEvent::FullyNegotiatedOutbound(FullyNegotiatedOutbound { .. }) => {}
            ConnectionEvent::DialUpgradeError(DialUpgradeError { .. }) => {}
            ConnectionEvent::ListenUpgradeError(ListenUpgradeError { .. }) => {}
            ConnectionEvent::AddressChange(_)
            | ConnectionEvent::LocalProtocolsChange(_)
            | ConnectionEvent::RemoteProtocolsChange(_) => {}
            _ => {}
        }
    }
}

#[derive(Debug)]
pub enum HandlerEvent {
    InboundAnnouncedPeer(PeerId),
}

#[derive(Clone)]
pub struct InboundProtocol {
    allow_inbound: bool,
}

impl InboundProtocol {
    fn new(allow_inbound: bool) -> Self {
        Self { allow_inbound }
    }
}

impl UpgradeInfo for InboundProtocol {
    type Info = libp2p::StreamProtocol;
    type InfoIter = iter::Once<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        iter::once(FUNGI_RELAY_REFRESH_PROTOCOL)
    }
}

impl InboundUpgrade<Stream> for InboundProtocol {
    type Output = Message;
    type Error = io::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_inbound(self, socket: Stream, _: Self::Info) -> Self::Future {
        Box::pin(async move {
            if !self.allow_inbound {
                let mut socket = socket;
                let _ = socket.close().await;
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "relay refresh peer is not trusted",
                ));
            }

            read_message(socket).await
        })
    }
}

#[derive(Clone)]
pub struct OutboundProtocol {
    message: Message,
}

impl OutboundProtocol {
    fn new(message: Message) -> Self {
        Self { message }
    }
}

impl UpgradeInfo for OutboundProtocol {
    type Info = libp2p::StreamProtocol;
    type InfoIter = iter::Once<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        iter::once(FUNGI_RELAY_REFRESH_PROTOCOL)
    }
}

impl OutboundUpgrade<Stream> for OutboundProtocol {
    type Output = ();
    type Error = io::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_outbound(self, socket: Stream, _: Self::Info) -> Self::Future {
        Box::pin(async move {
            send_message(socket, &self.message).await?;
            Ok(())
        })
    }
}

async fn read_message(mut socket: Stream) -> Result<Message, io::Error> {
    let mut bytes = Vec::new();
    socket.read_to_end(&mut bytes).await?;
    socket.close().await?;

    if bytes.len() > MAX_MESSAGE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "relay refresh message exceeds size limit",
        ));
    }

    bincode::deserialize(&bytes).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid relay refresh message: {error}"),
        )
    })
}

async fn send_message(mut socket: Stream, message: &Message) -> Result<(), io::Error> {
    let bytes = bincode::serialize(message).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("failed to encode relay refresh message: {error}"),
        )
    })?;

    if bytes.len() > MAX_MESSAGE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "relay refresh message exceeds size limit",
        ));
    }

    socket.write_all(&bytes).await?;
    socket.flush().await?;
    socket.close().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::{Result, anyhow};
    use libp2p::{
        Swarm, SwarmBuilder,
        core::{Multiaddr, multiaddr::Protocol},
        futures::StreamExt as _,
        identity::Keypair,
        noise,
        swarm::{NetworkBehaviour, SwarmEvent},
        tcp, yamux,
    };
    use std::net::Ipv4Addr;
    use std::time::Duration;

    #[derive(NetworkBehaviour)]
    struct TestBehaviour {
        relay_refresh: Behaviour,
    }

    fn build_swarm(relay_refresh: Behaviour) -> Result<Swarm<TestBehaviour>> {
        let keypair = Keypair::generate_ed25519();
        Ok(SwarmBuilder::with_existing_identity(keypair)
            .with_tokio()
            .with_tcp(
                tcp::Config::default(),
                noise::Config::new,
                yamux::Config::default,
            )?
            .with_behaviour(|_| TestBehaviour { relay_refresh })?
            .build())
    }

    async fn next_listen_addr(swarm: &mut Swarm<TestBehaviour>) -> Result<Multiaddr> {
        loop {
            let event = swarm
                .next()
                .await
                .ok_or_else(|| anyhow!("swarm ended while waiting for listen address"))?;
            if let SwarmEvent::NewListenAddr { address, .. } = event {
                return Ok(address);
            }
        }
    }

    async fn wait_all_connected(
        source: &mut Swarm<TestBehaviour>,
        relay: &mut Swarm<TestBehaviour>,
        target: &mut Swarm<TestBehaviour>,
        source_peer_id: PeerId,
        relay_peer_id: PeerId,
        target_peer_id: PeerId,
    ) -> Result<()> {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if source.is_connected(&relay_peer_id)
                    && relay.is_connected(&source_peer_id)
                    && target.is_connected(&relay_peer_id)
                    && relay.is_connected(&target_peer_id)
                {
                    return Ok(());
                }

                tokio::select! {
                    event = source.next() => event.ok_or_else(|| anyhow!("source swarm ended"))?,
                    event = relay.next() => event.ok_or_else(|| anyhow!("relay swarm ended"))?,
                    event = target.next() => event.ok_or_else(|| anyhow!("target swarm ended"))?,
                };
            }
        })
        .await
        .map_err(|_| anyhow!("timed out waiting for relay refresh smoke connections"))?
    }

    #[tokio::test]
    async fn relay_refresh_prepare_is_forwarded_to_target() -> Result<()> {
        let mut relay = build_swarm(Behaviour::new_allow_all())?;
        let relay_peer_id = *relay.local_peer_id();
        let mut source = build_swarm(Behaviour::new_allow_all())?;
        let source_peer_id = *source.local_peer_id();
        let mut target = build_swarm(Behaviour::new_trusted_relays([relay_peer_id]))?;
        let target_peer_id = *target.local_peer_id();

        relay.listen_on(
            Multiaddr::empty()
                .with(Protocol::from(Ipv4Addr::LOCALHOST))
                .with(Protocol::Tcp(0)),
        )?;
        let relay_addr = next_listen_addr(&mut relay).await?;
        let relay_addr = relay_addr
            .with_p2p(relay_peer_id)
            .map_err(|addr| anyhow!("failed to append relay peer id to {addr}"))?;

        source.dial(relay_addr.clone())?;
        target.dial(relay_addr)?;

        wait_all_connected(
            &mut source,
            &mut relay,
            &mut target,
            source_peer_id,
            relay_peer_id,
            target_peer_id,
        )
        .await?;

        source
            .behaviour_mut()
            .relay_refresh
            .send(&relay_peer_id, target_peer_id);

        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                tokio::select! {
                    event = source.next() => {
                        event.ok_or_else(|| anyhow!("source swarm ended"))?;
                    }
                    event = relay.next() => {
                        if let Some(SwarmEvent::Behaviour(TestBehaviourEvent::RelayRefresh(event))) = event {
                            assert_eq!(event.peer, source_peer_id);
                            assert_eq!(event.announced_peer_id, target_peer_id);
                            relay
                                .behaviour_mut()
                                .relay_refresh
                                .send(&event.announced_peer_id, event.peer);
                        } else if event.is_none() {
                            return Err(anyhow!("relay swarm ended"));
                        }
                    }
                    event = target.next() => {
                        if let Some(SwarmEvent::Behaviour(TestBehaviourEvent::RelayRefresh(event))) = event {
                            assert_eq!(event.peer, relay_peer_id);
                            assert_eq!(event.announced_peer_id, source_peer_id);
                            return Ok(());
                        } else if event.is_none() {
                            return Err(anyhow!("target swarm ended"));
                        }
                    }
                }
            }
        })
        .await
        .map_err(|_| anyhow!("timed out waiting for forwarded relay refresh notification"))?
    }
}
