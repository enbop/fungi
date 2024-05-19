use fungi_util::{AsyncBorrow, AsyncBorrowGuard, AsyncResult, Completer};
use libp2p::{
    futures::StreamExt,
    noise, ping,
    swarm::{dial_opts::DialOpts, DialError, NetworkInfo, SwarmEvent},
    tcp, yamux, PeerId, Swarm,
};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

type TSwarm = Swarm<ping::Behaviour>;
type SwarmGuard = AsyncBorrowGuard<TSwarm>;

pub struct SwarmState {
    #[allow(dead_code)]
    swarm_task: tokio::task::JoinHandle<()>,
    local_peer_id: PeerId,

    borrow_swarm_signal_tx: UnboundedSender<Completer<SwarmGuard>>,
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

        let (borrow_swarm_signal_tx, borrow_swarm_signal_rx) =
            unbounded_channel::<Completer<SwarmGuard>>();

        let local_peer_id = swarm.local_peer_id().to_owned();
        let swarm_task = tokio::spawn(Self::start_swarm_task(swarm, borrow_swarm_signal_rx));

        Ok(Self {
            swarm_task,
            local_peer_id,
            borrow_swarm_signal_tx,
        })
    }

    pub fn local_peer_id(&self) -> &PeerId {
        &self.local_peer_id
    }

    // TODO return Result
    async fn borrow_swarm(&self) -> SwarmGuard {
        AsyncResult::new(|completer| {
            self.borrow_swarm_signal_tx.send(completer).unwrap(); // TODO unwrap
        })
        .wait()
        .await
    }

    async fn start_swarm_task(
        mut swarm: Swarm<ping::Behaviour>,
        mut borrow_swarm_signal_rx: UnboundedReceiver<Completer<SwarmGuard>>,
    ) {
        loop {
            tokio::select! {
                swarm_events = swarm.select_next_some() => {
                    match swarm_events {
                        SwarmEvent::NewListenAddr { address, .. } => log::info!("Listening on {address:?}"),
                        SwarmEvent::Behaviour(event) => log::info!("{event:?}"),
                        _ => {}
                    }
                },
                borrow_request = borrow_swarm_signal_rx.recv() => {
                    let request = borrow_request.unwrap(); // TODO unwrap
                    swarm = AsyncBorrow::new(swarm).borrow(|swarm_guard| {
                        request.complete(swarm_guard);
                    }).await;
                },
            }
        }
    }
}

impl SwarmState {
    pub async fn network_info(&self) -> NetworkInfo {
        let swarm_guard = self.borrow_swarm().await;
        let lock = swarm_guard.lock();
        lock.network_info()
    }

    pub async fn dial(&mut self, opts: impl Into<DialOpts>) -> Result<(), DialError> {
        let swarm_guard = self.borrow_swarm().await;
        let mut lock = swarm_guard.lock();
        lock.dial(opts)
    }
}
