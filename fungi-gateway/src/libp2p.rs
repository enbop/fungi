use fungi_util::{AsyncResult, Completer};
use libp2p::{
    futures::StreamExt, noise, ping, swarm::SwarmEvent, tcp, yamux, Multiaddr, PeerId, Swarm,
};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

enum AsyncSwarmRequest {
    GetLocalPeerId(Completer<PeerId>),
}

pub struct SwarmState {
    async_swarm_caller: UnboundedSender<AsyncSwarmRequest>,
    swarm_task: tokio::task::JoinHandle<()>,
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

        let (async_swarm_request_tx, async_swarm_request_rx) =
            unbounded_channel::<AsyncSwarmRequest>();

        let swarm_task = tokio::spawn(Self::start_swarm_task(swarm, async_swarm_request_rx));

        Ok(Self {
            async_swarm_caller: async_swarm_request_tx,
            swarm_task,
        })
    }

    async fn start_swarm_task(
        mut swarm: Swarm<ping::Behaviour>,
        mut async_swarm_request_rx: tokio::sync::mpsc::UnboundedReceiver<AsyncSwarmRequest>,
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
                async_request = async_swarm_request_rx.recv() => {
                    let Some(async_request) = async_request else {
                        log::warn!("AsyncSwarmRequest channel closed");
                        break;
                    };
                    Self::handle_async_request(&mut swarm, async_request).await;
                }
            }
        }
    }

    async fn handle_async_request(
        swarm: &mut Swarm<ping::Behaviour>,
        async_call_swarm: AsyncSwarmRequest,
    ) {
        match async_call_swarm {
            AsyncSwarmRequest::GetLocalPeerId(completer) => {
                let local_peer_id = swarm.local_peer_id();
                completer.complete(local_peer_id.to_owned());
            }
        }
    }
}

impl SwarmState {
    pub async fn get_peer_id(&self) -> PeerId {
        AsyncResult::new(|completer| {
            self.async_swarm_caller
                .send(AsyncSwarmRequest::GetLocalPeerId(completer))
                .unwrap();
        })
        .wait()
        .await
    }
}
