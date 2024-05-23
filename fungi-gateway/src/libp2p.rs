use std::{path::PathBuf, sync::Arc, time::Duration};

use anyhow::Result;
use libp2p::{
    futures::StreamExt,
    identity::Keypair,
    noise, ping,
    swarm::{dial_opts::DialOpts, DialError, NetworkBehaviour, NetworkInfo, SwarmEvent},
    tcp, yamux, PeerId, StreamProtocol, Swarm,
};
use libp2p_stream::{AlreadyRegistered, IncomingStreams, OpenStreamError};
use tokio::sync::{Mutex, MutexGuard, Notify};

type TSwarm = Swarm<FungiBehaviours>;

#[derive(Clone)]
struct SwarmWrapper {
    ptr: Arc<Mutex<TSwarm>>,
    notify: Arc<Notify>,
}

impl SwarmWrapper {
    fn new(swarm: TSwarm) -> Self {
        Self {
            ptr: Arc::new(Mutex::new(swarm)),
            notify: Arc::new(Notify::new()),
        }
    }
}

pub struct SwarmState {
    #[allow(dead_code)]
    swarm_task: tokio::task::JoinHandle<()>,
    local_peer_id: PeerId,

    swarm: SwarmWrapper,
}

#[derive(NetworkBehaviour)]
struct FungiBehaviours {
    ping: ping::Behaviour,
    stream: libp2p_stream::Behaviour,
}

impl SwarmState {
    // TODO: error handling
    // TODO: configurable, consider using a builder pattern
    pub async fn start_libp2p_swarm(fungi_dir: &PathBuf) -> Result<Self> {
        let keypair = get_keypair_from_dir(fungi_dir)?;

        let mut swarm = libp2p::SwarmBuilder::with_existing_identity(keypair)
            .with_tokio()
            .with_tcp(
                tcp::Config::default(),
                noise::Config::new,
                yamux::Config::default,
            )?
            .with_quic()
            .with_behaviour(|_| FungiBehaviours {
                ping: ping::Behaviour::new(ping::Config::new()),
                stream: libp2p_stream::Behaviour::new(),
            })?
            .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(10)))
            .build();

        swarm.listen_on(
            "/ip4/0.0.0.0/tcp/0"
                .parse()
                .expect("address should be valid"),
        )?;
        swarm.listen_on(
            "/ip4/0.0.0.0/udp/0/quic-v1"
                .parse()
                .expect("address should be valid"),
        )?;

        let local_peer_id = swarm.local_peer_id().to_owned();
        let swarm_wrapper = SwarmWrapper::new(swarm);
        let swarm_task = tokio::spawn(Self::start_swarm_task(swarm_wrapper.clone()));

        Ok(Self {
            swarm_task,
            swarm: swarm_wrapper,
            local_peer_id,
        })
    }

    pub fn local_peer_id(&self) -> &PeerId {
        &self.local_peer_id
    }

    // TODO return Result
    async fn require_swarm(&self) -> MutexGuard<'_, TSwarm> {
        self.swarm.notify.notify_one();
        self.swarm.ptr.lock().await
    }

    async fn start_swarm_task(swarm: SwarmWrapper) {
        loop {
            swarm_loop(&swarm).await;
        }
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
                                log::info!("Listening on {addr:?}")
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

impl SwarmState {
    pub async fn network_info(&self) -> NetworkInfo {
        let swarm_guard = self.require_swarm().await;
        swarm_guard.network_info()
    }

    pub async fn dial(&mut self, opts: impl Into<DialOpts>) -> Result<(), DialError> {
        let mut swarm_guard = self.require_swarm().await;
        swarm_guard.dial(opts)
    }

    pub async fn stream_accept(
        &mut self,
        protocol: StreamProtocol,
    ) -> Result<IncomingStreams, AlreadyRegistered> {
        let mut swarm_guard = self.require_swarm().await;
        swarm_guard
            .behaviour_mut()
            .stream
            .new_control()
            .accept(protocol)
    }

    pub async fn stream_open(
        &mut self,
        peer: PeerId,
        protocol: StreamProtocol,
    ) -> Result<libp2p::Stream, OpenStreamError> {
        let mut swarm_guard = self.require_swarm().await;
        swarm_guard
            .behaviour_mut()
            .stream
            .new_control()
            .open_stream(peer, protocol)
            .await
    }
}

fn get_keypair_from_dir(fungi_dir: &PathBuf) -> Result<Keypair> {
    let keypair_file = fungi_dir.join(".keys").join("keypair");
    let encoded = std::fs::read(&keypair_file)?;
    let keypair = Keypair::from_protobuf_encoding(&encoded)?;
    Ok(keypair)
}
