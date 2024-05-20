use std::sync::Arc;

use libp2p::{
    futures::StreamExt,
    noise, ping,
    swarm::{dial_opts::DialOpts, DialError, NetworkInfo, SwarmEvent},
    tcp, yamux, PeerId, Swarm,
};
use tokio::sync::{Mutex, MutexGuard, Notify};

type TSwarm = Swarm<ping::Behaviour>;

#[derive(Clone)]
struct SwarmWrapper {
    ptr: Arc<Mutex<TSwarm>>,
    notify: Arc<Notify>,
}

pub struct SwarmState {
    #[allow(dead_code)]
    swarm_task: tokio::task::JoinHandle<()>,
    local_peer_id: PeerId,

    swarm: SwarmWrapper,
}

impl SwarmState {
    // TODO: error handling
    // TODO: configurable
    pub async fn start_libp2p_swarm() -> Result<Self, String> {
        let mut swarm = libp2p::SwarmBuilder::with_new_identity()
            .with_tokio()
            .with_tcp(
                tcp::Config::default(),
                noise::Config::new,
                yamux::Config::default,
            )
            .map_err(|e| format!("Failed to build swarm: {:?}", e))?
            .with_quic()
            .with_behaviour(|_| ping::Behaviour::default())
            .map_err(|e| format!("Failed to build swarm: {:?}", e))?
            .build();

        swarm
            .listen_on(
                "/ip4/0.0.0.0/tcp/0"
                    .parse()
                    .expect("address should be valid"),
            )
            .map_err(|e| format!("Failed to listen on address: {:?}", e))?;
        swarm
            .listen_on(
                "/ip4/0.0.0.0/udp/0/quic-v1"
                    .parse()
                    .expect("address should be valid"),
            )
            .map_err(|e| format!("Failed to listen on address: {:?}", e))?;

        let local_peer_id = swarm.local_peer_id().to_owned();
        let swarm_wrapper = SwarmWrapper {
            ptr: Arc::new(Mutex::new(swarm)),
            notify: Arc::new(Notify::new()),
        };
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
                            SwarmEvent::NewListenAddr { address, .. } => log::info!("Listening on {address:?}"),
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
}
