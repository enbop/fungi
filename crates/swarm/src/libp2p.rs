use crate::behaviours::FungiBehaviours;
use anyhow::{Result, bail};
use async_result::{AsyncResult, Completer};
use fungi_util::protocols::FUNGI_RELAY_HANDSHAKE_PROTOCOL;
use libp2p::{
    Multiaddr, PeerId, Swarm,
    futures::{AsyncReadExt, StreamExt},
    identity::Keypair,
    mdns,
    multiaddr::Protocol,
    noise,
    swarm::SwarmEvent,
    tcp, yamux,
};
use std::{
    any::Any,
    ops::{Deref, DerefMut},
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
    pub local_peer_id: Arc<PeerId>,
    pub swarm_caller_tx: UnboundedSender<SwarmAsyncCall>,
    pub stream_control: libp2p_stream::Control,
}

impl SwarmControl {
    pub fn local_peer_id(&self) -> PeerId {
        *self.local_peer_id
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
            .with_behaviour(|keypair, relay| FungiBehaviours::new(keypair, relay, mdns))?
            .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
            .build();

        let local_peer_id = *swarm.local_peer_id();
        let stream_control = swarm.behaviour().stream.new_control();

        apply(&mut swarm);

        let (swarm_caller_tx, swarm_caller_rx) = mpsc::unbounded_channel::<SwarmAsyncCall>();

        let task_handle = Self::spawn_swarm(swarm, swarm_caller_rx);

        Ok((
            SwarmControl {
                local_peer_id: Arc::new(local_peer_id),
                swarm_caller_tx,
                stream_control,
            },
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

        swarm_task
    }
}
