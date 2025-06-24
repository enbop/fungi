use crate::{behaviours::FungiBehaviours, peer_handshake::PeerHandshakePayload};
use anyhow::{Context, Result, bail};
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
use tokio::{
    sync::mpsc::{self, UnboundedReceiver, UnboundedSender},
    task::JoinHandle,
};

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

#[derive(Default, Clone)]
pub struct State {
    dial_callback: Arc<Mutex<HashMap<PeerId, Completer<std::result::Result<(), DialError>>>>>,
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

    pub fn dial_callback(
        &self,
    ) -> Arc<Mutex<HashMap<PeerId, Completer<std::result::Result<(), DialError>>>>> {
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

    state: State,
}

impl SwarmControl {
    pub fn new(
        local_peer_id: Arc<PeerId>,
        swarm_caller_tx: UnboundedSender<SwarmAsyncCall>,
        stream_control: libp2p_stream::Control,
        state: State,
    ) -> Self {
        Self {
            local_peer_id,
            swarm_caller_tx,
            stream_control,
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

    async fn handshake(&self, peer_id: PeerId) -> Result<()> {
        let mut stream = self
            .stream_control
            .clone()
            .open_stream(peer_id, FUNGI_PEER_HANDSHAKE_PROTOCOL)
            .await
            .context(format!("Failed to open handshake stream to {}", peer_id))?;
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
    // TODO handle error
    pub async fn connect(&self, peer_id: PeerId) -> std::result::Result<(), DialError> {
        if self.state.connected_peers.lock().contains_key(&peer_id) {
            return Ok(());
        }

        if self.state.dial_callback.lock().contains_key(&peer_id) {
            // TODO correct error type
            return Err(DialError::Aborted);
        }

        let (completer, res) = AsyncResult::new_split::<std::result::Result<(), DialError>>();

        let dial: std::result::Result<(), DialError> = self
            .invoke_swarm(move |swarm| {
                swarm.dial(peer_id.clone())?;
                swarm
                    .behaviour()
                    .dial_callback
                    .lock()
                    .insert(peer_id, completer);
                Ok(())
            })
            .await
            .map_err(|e| {
                log::warn!("Failed to invoke swarm for dial: {:?}", e);
                DialError::Aborted
            })?;
        dial?;
        res.await.map_err(|_| DialError::Aborted)??;

        self.handshake(peer_id)
            .await
            .map_err(|_| DialError::Aborted)?;
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

    pub async fn listen_relay(&self, relay_addr: Multiaddr) -> Result<()> {
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

        // 2. listen on relay
        self.invoke_swarm(|swarm| {
            swarm.listen_on(relay_addr.with(libp2p::multiaddr::Protocol::P2pCircuit))
        })
        .await??;
        Ok(())
    }
}

pub struct FungiSwarm;

impl FungiSwarm {
    pub async fn start_swarm(
        keypair: Keypair,
        state: State,
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
                                let addr = address.with_p2p(*swarm.local_peer_id()).unwrap();
                                println!("Listening on {addr:?}")
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
                                log::info!("Connection closed with {peer_id:?}: {:?}", cause);
                                // update connected peers
                                swarm.behaviour().connected_peers.lock().remove(&peer_id);
                            },
                            // SwarmEvent::Behaviour(event) => log::info!("{event:?}"),
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
