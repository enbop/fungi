use fungi_util::{AsyncResult, Completer};
use libp2p::{
    futures::StreamExt, noise, ping, swarm::SwarmEvent, tcp, yamux, Multiaddr, PeerId, Swarm,
};
use std::{io, sync::OnceLock};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

// TODO: only start the swarm once
static ASYNC_SWARM_CALLER: OnceLock<UnboundedSender<AsyncSwarmRequests>> = OnceLock::new();

enum AsyncSwarmRequests {
    GetLocalPeerId(Completer<PeerId>),
}

// TODO: error handling
pub async fn start_libp2p_swarm() {
    let mut swarm = libp2p::SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )
        .unwrap()
        .with_behaviour(|_| ping::Behaviour::default())
        .unwrap()
        .build();

    swarm
        .listen_on("/ip4/0.0.0.0/tcp/0".parse().unwrap())
        .unwrap();

    let (async_swarm_request_tx, mut async_swarm_request_rx) =
        unbounded_channel::<AsyncSwarmRequests>();

    ASYNC_SWARM_CALLER.set(async_swarm_request_tx).unwrap();

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
                handle_async_request(&mut swarm, async_request).await;
            }
        }
    }
}

async fn handle_async_request(
    swarm: &mut Swarm<ping::Behaviour>,
    async_call_swarm: AsyncSwarmRequests,
) {
    match async_call_swarm {
        AsyncSwarmRequests::GetLocalPeerId(completer) => {
            let local_peer_id = swarm.local_peer_id();
            completer.complete(local_peer_id.to_owned());
        }
    }
}

pub struct AsyncSwarmRequest;
use AsyncSwarmRequests::*;

impl AsyncSwarmRequest {
    pub async fn get_peer_id() -> PeerId {
        AsyncResult::new(|completer| {
            ASYNC_SWARM_CALLER
                .get()
                .unwrap()
                .send(GetLocalPeerId(completer))
                .unwrap();
        })
        .wait()
        .await
    }
}
