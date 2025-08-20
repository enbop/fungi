use crate::{behaviours::FungiBehaviours, peer_handshake::PeerHandshakePayload};
use anyhow::{Result, bail};
use async_result::{AsyncResult, Completer};
use fungi_util::protocols::{FUNGI_PEER_HANDSHAKE_PROTOCOL, FUNGI_RELAY_HANDSHAKE_PROTOCOL};
use libp2p::{
    Multiaddr, PeerId, Swarm,
    futures::{AsyncReadExt, AsyncWriteExt, StreamExt},
    identity::Keypair,
    mdns,
    multiaddr::Protocol,
    noise,
    swarm::{DialError, SwarmEvent},
    tcp, yamux,
};
use parking_lot::{Mutex, RwLock};
use std::{
    any::Any,
    collections::{HashMap, HashSet},
    ops::Deref,
    sync::Arc,
    time::Duration,
};
use thiserror::Error;
use tokio::{
    sync::mpsc::{self, UnboundedReceiver, UnboundedSender},
    task::JoinHandle,
};

#[derive(Error, Debug)]
pub enum ConnectError {
    #[error("Dial failed: {0}")]
    DialFailed(#[from] DialError),
    #[error("Already dialing peer {peer_id}")]
    AlreadyDialing { peer_id: PeerId },
    #[error("Swarm invocation failed: {0}")]
    SwarmInvocationFailed(anyhow::Error),
    #[error("Handshake failed: {0}")]
    HandshakeFailed(anyhow::Error),
    #[error("Connection cancelled")]
    Cancelled,
}

pub type TSwarm = Swarm<FungiBehaviours>;
type SwarmResponse = Box<dyn Any + Send>;
type SwarmRequest = Box<dyn FnOnce(&mut TSwarm) -> SwarmResponse + Send + Sync>;

pub struct ConnectedPeer {
    handshake: Option<PeerHandshakePayload>,
    multiaddr: Multiaddr,
}

impl ConnectedPeer {
    pub fn with_multiaddr(multiaddr: Multiaddr) -> Self {
        Self {
            handshake: None,
            multiaddr,
        }
    }

    pub fn update_handshake(&mut self, handshake: PeerHandshakePayload) {
        self.handshake = Some(handshake);
    }

    pub fn host_name(&self) -> Option<String> {
        self.handshake.as_ref().and_then(|h| h.host_name())
    }

    pub fn multiaddr(&self) -> &Multiaddr {
        &self.multiaddr
    }
}

type DialCallback = Arc<Mutex<HashMap<PeerId, Completer<std::result::Result<(), DialError>>>>>;

#[derive(Default, Clone)]
pub struct State {
    dial_callback: DialCallback,
    connected_peers: Arc<Mutex<HashMap<PeerId, ConnectedPeer>>>,
    incoming_allowed_peers: Arc<RwLock<HashSet<PeerId>>>,
}

impl State {
    pub fn new(incoming_allowed_peers: HashSet<PeerId>) -> Self {
        Self {
            dial_callback: Arc::new(Mutex::new(HashMap::new())),
            connected_peers: Arc::new(Mutex::new(HashMap::new())),
            incoming_allowed_peers: Arc::new(RwLock::new(incoming_allowed_peers)),
        }
    }

    pub fn dial_callback(&self) -> DialCallback {
        self.dial_callback.clone()
    }

    pub fn connected_peers(&self) -> Arc<Mutex<HashMap<PeerId, ConnectedPeer>>> {
        self.connected_peers.clone()
    }

    pub fn incoming_allowed_peers(&self) -> Arc<RwLock<HashSet<PeerId>>> {
        self.incoming_allowed_peers.clone()
    }

    pub fn get_incoming_allowed_peers_list(&self) -> Vec<PeerId> {
        self.incoming_allowed_peers.read().iter().cloned().collect()
    }
}

pub struct SwarmAsyncCall {
    request: SwarmRequest,
    response: Completer<SwarmResponse>,
}

impl SwarmAsyncCall {
    pub fn new(request: SwarmRequest, response: Completer<SwarmResponse>) -> Self {
        Self { request, response }
    }
}

impl Deref for SwarmControl {
    type Target = UnboundedSender<SwarmAsyncCall>;

    fn deref(&self) -> &Self::Target {
        &self.swarm_caller_tx
    }
}

#[derive(Clone)]
pub struct SwarmControl {
    local_peer_id: Arc<PeerId>,
    swarm_caller_tx: UnboundedSender<SwarmAsyncCall>,
    stream_control: libp2p_stream::Control,
    relay_addresses: Arc<Vec<Multiaddr>>,

    state: State,
}

impl SwarmControl {
    pub fn new(
        local_peer_id: Arc<PeerId>,
        swarm_caller_tx: UnboundedSender<SwarmAsyncCall>,
        stream_control: libp2p_stream::Control,
        relay_addresses: Arc<Vec<Multiaddr>>,
        state: State,
    ) -> Self {
        Self {
            local_peer_id,
            swarm_caller_tx,
            stream_control,
            relay_addresses,
            state,
        }
    }

    pub fn local_peer_id(&self) -> PeerId {
        *self.local_peer_id
    }

    pub fn stream_control(&self) -> &libp2p_stream::Control {
        &self.stream_control
    }

    pub fn stream_control_mut(&mut self) -> &mut libp2p_stream::Control {
        &mut self.stream_control
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    // TODO impl handshake
    async fn _handshake(&self, peer_id: PeerId) -> Result<()> {
        let mut stream = self
            .stream_control
            .clone()
            .open_stream(peer_id, FUNGI_PEER_HANDSHAKE_PROTOCOL)
            .await
            .map_err(|e| ConnectError::HandshakeFailed(anyhow::anyhow!(e)))?;
        stream
            .write_all(&PeerHandshakePayload::new().to_bytes())
            .await?;
        let mut buf = [0; 512];
        let n = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut buf)).await??;
        let handshake_res = PeerHandshakePayload::from_bytes(&buf[..n])?;
        log::info!(
            "Connected to {} - {}",
            handshake_res.host_name().unwrap_or_default(),
            peer_id
        );
        self.state
            .connected_peers
            .lock()
            .get_mut(&peer_id)
            .expect("Peer should be connected")
            .update_handshake(handshake_res);

        Ok(())
    }

    // connect and handshake
    // TODO add a timeout
    pub async fn connect(&self, peer_id: PeerId) -> Result<(), ConnectError> {
        if self.state.connected_peers.lock().contains_key(&peer_id) {
            return Ok(());
        }

        if self.state.dial_callback.lock().contains_key(&peer_id) {
            log::warn!("Already dialing {peer_id}");
            return Err(ConnectError::AlreadyDialing { peer_id });
        }

        let (completer, res) = AsyncResult::new_split::<std::result::Result<(), DialError>>();

        let relay_addresses = self.relay_addresses.clone();
        let dial_result = self
            .invoke_swarm(move |swarm| {
                if swarm.is_connected(&peer_id) {
                    log::info!("Already connected to {peer_id}");
                    completer.complete(Ok(()));
                    return Ok(());
                }
                if let Err(e) = swarm.dial(peer_id) {
                    match e {
                        DialError::NoAddresses => {
                            if relay_addresses.is_empty() {
                                log::warn!("No addresses to dial {peer_id} and no relay addresses available");
                                return Err(DialError::NoAddresses);
                            }
                            // TODO: add a rendezvous server
                            // dial with relay when no mDNS addresses are available
                            log::info!(
                                "Dialing {peer_id} with relay address {:?}",
                                relay_addresses
                            );
                            for relay_addr in relay_addresses.iter() {
                                swarm.add_peer_address(
                                    peer_id,
                                    peer_addr_with_relay(peer_id, relay_addr.clone()),
                                );
                            }
                            swarm.dial(peer_id)?;
                        }
                        _ => return Err(e),
                    }
                };
                swarm
                    .behaviour()
                    .dial_callback
                    .lock()
                    .insert(peer_id, completer);
                Ok(())
            })
            .await;

        match dial_result {
            Ok(dial_res) => dial_res?,
            Err(e) => {
                log::warn!("Failed to invoke swarm for dial: {e:?}");
                return Err(ConnectError::SwarmInvocationFailed(e));
            }
        }

        // Wait for dial result
        res.await.map_err(|_| ConnectError::Cancelled)??;

        // TODO impl handshake
        // self.handshake(peer_id)
        //     .await
        //     .map_err(ConnectError::HandshakeFailed)?;

        Ok(())
    }

    pub async fn invoke_swarm<F, R: Any + Send>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&mut TSwarm) -> R + Send + Sync + 'static,
    {
        let res = AsyncResult::with(move |completer| {
            self.send(SwarmAsyncCall::new(
                Box::new(|swarm| Box::new(f(swarm))),
                completer,
            ))
            .ok(); // should be ok cause the completer will be dropped if the channel is closed
        })
        .await
        .map_err(|e| anyhow::anyhow!("Swarm call failed: {:?}", e))?
        .downcast::<R>()
        .map_err(|_| anyhow::anyhow!("Swarm call failed: downcast error"))?;
        Ok(*res)
    }

    pub fn relay_addresses(&self) -> Arc<Vec<Multiaddr>> {
        self.relay_addresses.clone()
    }

    pub async fn listen_relay(&self) {
        for relay_addr in self.relay_addresses.iter() {
            if let Err(e) = self.listen_relay_by_addr(relay_addr.clone()).await {
                log::error!("Failed to listen on relay address {relay_addr:?}: {e:?}");
            }
        }
    }

    async fn listen_relay_by_addr(&self, relay_addr: Multiaddr) -> Result<()> {
        let relay_peer = relay_addr
            .iter()
            .find_map(|p| {
                if let Protocol::P2p(peer_id) = p {
                    Some(peer_id)
                } else {
                    None
                }
            })
            .ok_or_else(|| anyhow::anyhow!("Invalid relay address"))?;

        // 1. handshake with relay
        // https://github.com/libp2p/rust-libp2p/blob/9a45db3f82b760c93099e66ec77a7a772d1f6cd3/examples/dcutr/src/main.rs#L139
        // Connect to the relay server. Not for the reservation or relayed connection, but to (a) learn
        // our local public address and (b) enable a freshly started relay to learn its public address.
        let relay_addr_cl = relay_addr.clone();
        self.invoke_swarm(|swarm| swarm.dial(relay_addr_cl))
            .await??;
        let Ok(stream_result) = tokio::time::timeout(
            Duration::from_secs(3),
            self.stream_control
                .clone()
                .open_stream(relay_peer, FUNGI_RELAY_HANDSHAKE_PROTOCOL),
        )
        .await
        else {
            bail!("Handshake timeout")
        };
        let mut stream = match stream_result {
            Ok(stream) => stream,
            Err(e) => bail!("Handshake failed: {:?}", e),
        };
        let mut buf = [0u8; 32];
        // TODO
        // implement a proper handshake protocol, currently just read the response to make sure both sides are reachable
        let n = stream.read(&mut buf).await?;
        if n < 1 {
            bail!("Handshake failed: empty response");
        };

        println!("Listening on relay address: {relay_addr:?}");
        // 2. listen on relay
        self.invoke_swarm(|swarm| swarm.listen_on(relay_addr.with(Protocol::P2pCircuit)))
            .await??;

        Ok(())
    }
}

pub struct FungiSwarm;

impl FungiSwarm {
    pub async fn start_swarm(
        keypair: Keypair,
        state: State,
        relay_addresses: Vec<Multiaddr>,
        apply: impl FnOnce(&mut TSwarm),
    ) -> Result<(SwarmControl, JoinHandle<()>)> {
        let mdns =
            mdns::tokio::Behaviour::new(mdns::Config::default(), keypair.public().to_peer_id())?;

        let mut swarm = libp2p::SwarmBuilder::with_existing_identity(keypair)
            .with_tokio()
            .with_tcp(
                tcp::Config::default(),
                noise::Config::new,
                yamux::Config::default,
            )?
            .with_quic()
            .with_relay_client(noise::Config::new, yamux::Config::default)?
            .with_behaviour(|keypair, relay| {
                FungiBehaviours::new(keypair, relay, mdns, state.clone())
            })?
            .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
            .build();

        let local_peer_id = *swarm.local_peer_id();
        let stream_control = swarm.behaviour().stream.new_control();

        apply(&mut swarm);

        let (swarm_caller_tx, swarm_caller_rx) = mpsc::unbounded_channel::<SwarmAsyncCall>();

        let task_handle = Self::spawn_swarm(swarm, swarm_caller_rx);

        Ok((
            SwarmControl::new(
                Arc::new(local_peer_id),
                swarm_caller_tx,
                stream_control,
                Arc::new(relay_addresses),
                state,
            ),
            task_handle,
        ))
    }

    pub fn spawn_swarm(
        swarm: TSwarm,
        swarm_caller_rx: UnboundedReceiver<SwarmAsyncCall>,
    ) -> JoinHandle<()> {
        let swarm_task = tokio::spawn(swarm_loop(swarm, swarm_caller_rx));
        async fn swarm_loop(
            mut swarm: TSwarm,
            mut swarm_caller_rx: UnboundedReceiver<SwarmAsyncCall>,
        ) {
            loop {
                tokio::select! {
                    swarm_events = swarm.select_next_some() => {
                        match swarm_events {
                            SwarmEvent::NewListenAddr { address, .. } => {
                                println!("Listening on {address:?}");
                            },
                            SwarmEvent::NewExternalAddrCandidate { address, .. } => {
                                log::info!("New external address candidate: {address:?}");
                            },
                            SwarmEvent::ConnectionEstablished { peer_id, endpoint,.. } => {
                                log::info!("Connection established with {peer_id:?} at {endpoint:?}");
                                // check dial callback
                                if let Some(completer) = swarm.behaviour().dial_callback.lock().remove(&peer_id) {
                                    completer.complete(Ok(()));
                                }
                                // add peer to connected peers
                                swarm.behaviour().connected_peers.lock().insert(peer_id,
                                    ConnectedPeer::with_multiaddr(
                                        endpoint.get_remote_address().clone()
                                    ));
                            },
                            SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                                log::info!("Outgoing connection error with {peer_id:?}: {error:?}");
                                // check dial callback
                                let Some(peer_id) = peer_id else {
                                    continue;
                                };
                                if let Some(completer) = swarm.behaviour().dial_callback.lock().remove(&peer_id) {
                                    completer.complete(Err(error));
                                }
                            },
                            SwarmEvent::ConnectionClosed { peer_id, cause, .. } => {
                                log::info!("Connection closed with {peer_id:?}: {cause:?}");
                                // update connected peers
                                swarm.behaviour().connected_peers.lock().remove(&peer_id);
                            },
                            _ => {}
                        }
                    },
                    invoke = swarm_caller_rx.recv() => {
                        let Some(SwarmAsyncCall{ request, response }) = invoke else {
                            log::debug!("Swarm caller channel closed");
                            break;
                        };
                        let res = request(&mut swarm);
                        response.complete(res);
                    }
                }
            }
            log::info!("Swarm loop exited");
        }

        swarm_task
    }
}

pub fn get_default_relay_addrs() -> Vec<Multiaddr> {
    vec![
        "/ip4/160.16.206.21/tcp/30001/p2p/16Uiu2HAmGXFS6aYsKKYRkEDo1tNigZKN8TAYrsfSnEdC5sZLNkiE"
            .parse()
            .unwrap(),
        "/ip4/160.16.206.21/udp/30001/quic-v1/p2p/16Uiu2HAmGXFS6aYsKKYRkEDo1tNigZKN8TAYrsfSnEdC5sZLNkiE"
            .parse()
            .unwrap(),
    ]
}

pub fn peer_addr_with_relay(peer_id: PeerId, relay: Multiaddr) -> Multiaddr {
    relay
        .with(Protocol::P2pCircuit)
        .with(Protocol::P2p(peer_id))
}
