use std::{
    ops::{Deref, DerefMut},
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::Result;
use libp2p::{
    futures::{stream, StreamExt},
    identity::Keypair,
    mdns, noise, ping,
    swarm::{dial_opts::DialOpts, DialError, NetworkBehaviour, NetworkInfo, SwarmEvent},
    tcp, yamux, Multiaddr, PeerId, StreamProtocol, Swarm,
};
use libp2p_stream::{AlreadyRegistered, IncomingStreams, OpenStreamError};
use tokio::sync::{Mutex as TokioMutex, MutexGuard as TokioMutexGuard, Notify};

use crate::address_book;

pub type TSwarm = Swarm<FungiBehaviours>;

#[derive(Clone)]
pub struct SwarmWrapper {
    ptr: Arc<TokioMutex<TSwarm>>,
    notify: Arc<Notify>,
    local_peer_id: Arc<PeerId>,

    stream_control: libp2p_stream::Control,
}

impl SwarmWrapper {
    fn new(swarm: TSwarm) -> Self {
        let peer_id = swarm.local_peer_id().to_owned();
        let stream_control = swarm.behaviour().stream.new_control();
        Self {
            ptr: Arc::new(TokioMutex::new(swarm)),
            notify: Arc::new(Notify::new()),
            local_peer_id: Arc::new(peer_id),
            stream_control,
        }
    }

    pub fn local_peer_id(&self) -> &PeerId {
        &self.local_peer_id
    }
}

pub struct SwarmState {
    swarm: SwarmWrapper,
    swarm_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

#[derive(NetworkBehaviour)]
pub struct FungiBehaviours {
    ping: ping::Behaviour,
    pub stream: libp2p_stream::Behaviour,
    mdns: mdns::tokio::Behaviour,
    pub address_book: address_book::Behaviour,
}

impl SwarmState {
    // TODO: error handling
    pub async fn new(fungi_dir: &Path, apply: impl FnOnce(&mut TSwarm)) -> Result<Self> {
        let keypair = get_keypair_from_dir(fungi_dir)?;

        let mut swarm = libp2p::SwarmBuilder::with_existing_identity(keypair)
            .with_tokio()
            .with_tcp(
                tcp::Config::default(),
                noise::Config::new,
                yamux::Config::default,
            )?
            .with_quic()
            .with_behaviour(|key| FungiBehaviours {
                ping: ping::Behaviour::new(ping::Config::new()),
                stream: libp2p_stream::Behaviour::new(),
                mdns: mdns::tokio::Behaviour::new(
                    mdns::Config::default(),
                    key.public().to_peer_id(),
                )
                .unwrap(), // TODO if-watch unwrap
                address_book: Default::default(),
            })?
            .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(10)))
            .build();

        apply(&mut swarm);

        let swarm_wrapper = SwarmWrapper::new(swarm);

        Ok(Self {
            swarm: swarm_wrapper,
            swarm_task: Default::default(),
        })
    }

    pub fn local_peer_id(&self) -> &PeerId {
        &self.local_peer_id
    }

    pub fn is_started(&self) -> bool {
        self.swarm_task.lock().unwrap().is_some()
    }

    pub fn start_swarm_task(&mut self) {
        let mut swarm_task_lock = self.swarm_task.lock().unwrap();
        if swarm_task_lock.is_some() {
            return;
        }

        let swarm = self.swarm.clone();
        *swarm_task_lock = Some(tokio::spawn(async move {
            loop {
                swarm_loop(&swarm).await;
            }
        }));

        async fn swarm_loop(swarm: &SwarmWrapper) {
            let mut swarm_lock = swarm.ptr.lock().await;
            loop {
                tokio::select! {
                    biased;
                    swarm_events = swarm_lock.select_next_some() => {
                        log::debug!("Handle swarm event {:?}", swarm_events);
                        match swarm_events {
                            SwarmEvent::NewListenAddr { address, .. } => {
                                let addr = address.with_p2p(*swarm_lock.local_peer_id()).unwrap();
                                println!("Listening on {addr:?}")
                            },
                            SwarmEvent::Behaviour(event) => log::info!("{event:?}"),
                            _ => {}
                        }
                    },
                    // release the lock
                    _ = swarm.notify.notified() => {
                        break;
                    },
                }
            }
        }
    }
}

impl SwarmWrapper {
    // TODO return Result
    async fn require_swarm(&self) -> TokioMutexGuard<'_, TSwarm> {
        self.notify.notify_one();
        self.ptr.lock().await
    }

    pub async fn network_info(&self) -> NetworkInfo {
        let swarm_guard = self.require_swarm().await;
        swarm_guard.network_info()
    }

    pub async fn add_peer_addresses(
        &mut self,
        peer_id: PeerId,
        addrs: impl IntoIterator<Item = Multiaddr>,
    ) {
        let mut swarm_guard = self.require_swarm().await;
        for addr in addrs {
            swarm_guard.add_peer_address(peer_id, addr);
        }
    }

    pub async fn dial(&mut self, opts: impl Into<DialOpts>) -> Result<(), DialError> {
        let mut swarm_guard = self.require_swarm().await;
        swarm_guard.dial(opts)
    }

    pub fn new_stream_control(&mut self) -> libp2p_stream::Control {
        self.stream_control.clone()
    }

    pub fn stream_accept(
        &mut self,
        protocol: StreamProtocol,
    ) -> Result<IncomingStreams, AlreadyRegistered> {
        self.stream_control.accept(protocol)
    }

    pub async fn stream_open(
        &mut self,
        peer: PeerId,
        protocol: StreamProtocol,
    ) -> Result<libp2p::Stream, OpenStreamError> {
        self.stream_control.open_stream(peer, protocol).await
    }
}

impl Deref for SwarmState {
    type Target = SwarmWrapper;

    fn deref(&self) -> &Self::Target {
        &self.swarm
    }
}

impl DerefMut for SwarmState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.swarm
    }
}

fn get_keypair_from_dir(fungi_dir: &Path) -> Result<Keypair> {
    let keypair_file = fungi_dir.join(".keys").join("keypair");
    let encoded = std::fs::read(keypair_file)?;
    let keypair = Keypair::from_protobuf_encoding(&encoded)?;
    Ok(keypair)
}
