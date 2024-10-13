use crate::behaviours::FungiBehaviours;
use anyhow::{bail, Result};
use async_result::{AsyncResult, Completer};
use fungi_util::protocols::FUNGI_RELAY_HANDSHAKE_PROTOCOL;
use libp2p::{
    futures::{AsyncReadExt, StreamExt},
    identity::Keypair,
    mdns,
    multiaddr::Protocol,
    noise,
    swarm::SwarmEvent,
    tcp, yamux, Multiaddr, PeerId, Swarm,
};
use std::{
    any::Any,
    ops::{Deref, DerefMut},
    time::Duration,
};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

pub type TSwarm = Swarm<FungiBehaviours>;
type SwarmResponse = Box<dyn Any + Send>;
type SwarmRequest = Box<dyn FnOnce(&mut TSwarm) -> SwarmResponse + Send + Sync>;

pub struct SwarmAsyncCall {
    request: SwarmRequest,
    response: Completer<SwarmResponse>,
}

impl SwarmAsyncCall {
    pub fn new(request: SwarmRequest, response: Completer<SwarmResponse>) -> Self {
        Self { request, response }
    }
}

pub struct SwarmStateRunning {
    // TODO use tokio::task::JoinHandle<TSwarm>
    _task: tokio::task::JoinHandle<()>,
    swarm_caller_tx: UnboundedSender<SwarmAsyncCall>,
}

impl SwarmStateRunning {
    pub fn new(
        _task: tokio::task::JoinHandle<()>,
        swarm_caller_tx: UnboundedSender<SwarmAsyncCall>,
    ) -> Self {
        Self {
            _task,
            swarm_caller_tx,
        }
    }
}

impl Deref for SwarmStateRunning {
    type Target = UnboundedSender<SwarmAsyncCall>;

    fn deref(&self) -> &Self::Target {
        &self.swarm_caller_tx
    }
}

impl DerefMut for SwarmStateRunning {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.swarm_caller_tx
    }
}

pub enum SwarmState {
    Running(SwarmStateRunning),
    Uninitialized(Option<TSwarm>),
}

impl SwarmState {
    pub fn is_running(&self) -> bool {
        matches!(self, SwarmState::Running(_))
    }
}

pub struct FungiSwarm {
    local_peer_id: PeerId,
    swarm_state: SwarmState,
    pub stream_control: libp2p_stream::Control,
}

impl FungiSwarm {
    pub async fn new(keypair: Keypair, apply: impl FnOnce(&mut TSwarm)) -> Result<Self> {
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
            .with_behaviour(|keypair, relay| FungiBehaviours::new(keypair, relay, mdns))?
            .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
            .build();

        let local_peer_id = *swarm.local_peer_id();
        let stream_control = swarm.behaviour().stream.new_control();

        apply(&mut swarm);

        let swarm_state = SwarmState::Uninitialized(Some(swarm));

        Ok(Self {
            swarm_state,
            local_peer_id,
            stream_control,
        })
    }

    pub fn local_peer_id(&self) -> &PeerId {
        &self.local_peer_id
    }

    pub fn is_started(&self) -> bool {
        self.swarm_state.is_running()
    }

    pub async fn invoke_swarm<F, R: Any + Send>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&mut TSwarm) -> R + Send + Sync + 'static,
    {
        let SwarmState::Running(swarm_state) = &self.swarm_state else {
            bail!("Swarm not started")
        };
        let res = AsyncResult::with(move |completer| {
            swarm_state
                .send(SwarmAsyncCall::new(
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

    pub fn start_swarm_task(&mut self) {
        let (swarm_caller_tx, swarm_caller_rx) = mpsc::unbounded_channel::<SwarmAsyncCall>();

        let swarm = {
            let SwarmState::Uninitialized(swarm) = &mut self.swarm_state else {
                log::warn!("Swarm already started");
                return;
            };
            let Some(swarm) = swarm.take() else {
                log::warn!("Wrong swarm state"); // expected to be unreachable
                return;
            };
            swarm
        };
        let swarm_task = tokio::spawn(swarm_loop(swarm, swarm_caller_rx));
        self.swarm_state = SwarmState::Running(SwarmStateRunning::new(swarm_task, swarm_caller_tx));

        async fn swarm_loop(
            mut swarm: TSwarm,
            mut swarm_caller_rx: UnboundedReceiver<SwarmAsyncCall>,
        ) {
            loop {
                tokio::select! {
                    swarm_events = swarm.select_next_some() => {
                        log::debug!("Handle swarm event {:?}", swarm_events);
                        match swarm_events {
                            SwarmEvent::NewListenAddr { address, .. } => {
                                let addr = address.with_p2p(*swarm.local_peer_id()).unwrap();
                                println!("Listening on {addr:?}")
                            },
                            SwarmEvent::Behaviour(event) => log::info!("{event:?}"),
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
    }

    pub async fn listen_relay(&mut self, relay_addr: Multiaddr) -> Result<()> {
        if !self.is_started() {
            bail!("Swarm not started");
        }
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
