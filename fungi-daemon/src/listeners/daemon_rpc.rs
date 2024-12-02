use fungi_config::FungiDir;
use fungi_swarm::SwarmController;
use fungi_util::ipc;
use futures::StreamExt;
use interprocess::local_socket::tokio::prelude::*;
use std::{future::Future, io};

use libp2p::PeerId;
use tarpc::{
    context::Context,
    serde_transport as transport,
    server::{BaseChannel, Channel},
    tokio_serde::formats::Bincode,
};
use tokio::task::JoinHandle;
use tokio_util::codec::LengthDelimitedCodec;

use crate::DaemonArgs;

#[tarpc::service]
pub trait FungiDaemonRpc {
    async fn peer_id() -> PeerId;
}

#[derive(Clone)]
pub struct FungiDaemonRpcServer {
    swarm_controller: SwarmController,
}

impl FungiDaemonRpcServer {
    pub fn start(
        args: DaemonArgs,
        swarm_controller: SwarmController,
    ) -> io::Result<JoinHandle<()>> {
        let ipc_listener = ipc::create_ipc_listener(&args.daemon_rpc_path().to_string_lossy())?;
        let task_handle = tokio::spawn(Self::listen_from_ipc(
            Self { swarm_controller },
            ipc_listener,
        ));
        Ok(task_handle)
    }
}

impl FungiDaemonRpc for FungiDaemonRpcServer {
    async fn peer_id(self, _: Context) -> PeerId {
        self.swarm_controller.local_peer_id()
    }
}

impl FungiDaemonRpcServer {
    pub async fn listen_from_ipc(self, ipc_listener: LocalSocketListener) {
        let codec_builder = LengthDelimitedCodec::builder();

        async fn spawn(fut: impl Future<Output = ()> + Send + 'static) {
            tokio::spawn(fut);
        }

        loop {
            let stream = ipc_listener.accept().await.unwrap();
            let framed = codec_builder.new_framed(stream);
            let transport = transport::new(framed, Bincode::default());

            let this = self.clone();
            let fut = BaseChannel::with_defaults(transport)
                .execute(this.serve())
                .for_each(spawn);
            tokio::spawn(fut);
        }
    }
}
