use fungi_swarm::SwarmControl;
use futures::StreamExt;
use interprocess::local_socket::tokio::prelude::*;
use libp2p::{PeerId, StreamProtocol};
use std::{
    collections::HashMap,
    future::Future,
    io,
    sync::{Arc, Mutex},
};
use tarpc::{
    context::Context,
    serde_transport as transport,
    server::{BaseChannel, Channel},
    tokio_serde::formats::Bincode,
};
use tokio::{io::AsyncWriteExt, task::JoinHandle};
use tokio_util::{codec::LengthDelimitedCodec, compat::FuturesAsyncReadCompatExt as _};

use crate::DaemonArgs;

#[tarpc::service]
pub trait FungiDaemonRpc {
    async fn peer_id() -> PeerId;

    async fn accept_stream(protocol: String, ipc_name: String) -> Result<(), String>;

    async fn close_accepting_stream(protocol: String) -> Result<(), String>;
}

#[derive(Clone)]
pub struct FungiDaemonRpcServer {
    swarm_control: SwarmControl,

    accept_streams: Arc<Mutex<HashMap<StreamProtocol, JoinHandle<()>>>>,
}

impl FungiDaemonRpcServer {
    pub fn start(_args: DaemonArgs, _swarm_control: SwarmControl) -> io::Result<JoinHandle<()>> {
        // TODO release the sock address if it already exists
        // let ipc_listener = ipc::create_ipc_listener(&args.daemon_rpc_path().to_string_lossy())?;
        // let task_handle = tokio::spawn(Self::listen_from_ipc(
        //     Self {
        //         swarm_control,
        //         accept_streams: Default::default(),
        //     },
        //     ipc_listener,
        // ));
        Ok(tokio::spawn(async {}))
    }
}

impl FungiDaemonRpc for FungiDaemonRpcServer {
    async fn peer_id(self, _: Context) -> PeerId {
        self.swarm_control.local_peer_id()
    }

    async fn accept_stream(
        mut self,
        _: Context,
        protocol: String,
        ipc_name: String,
    ) -> Result<(), String> {
        let protocol = StreamProtocol::try_from_owned(protocol).map_err(|e| e.to_string())?;

        let mut streams_map_lock = self.accept_streams.lock().unwrap();
        if streams_map_lock.contains_key(&protocol) {
            return Err("Stream already exists".to_string());
        }

        let mut incoming_streams = self
            .swarm_control
            .stream_control_mut()
            .accept(protocol.clone())
            .map_err(|e| e.to_string())?;

        let task = tokio::spawn(async move {
            loop {
                let Some((peer_id, libp2p_stream)) = incoming_streams.next().await else {
                    break;
                };

                let Ok(mut local_stream) = fungi_util::ipc::connect_ipc(&ipc_name).await else {
                    println!("Failed to connect to ipc stream");
                    break;
                };
                tokio::spawn(async move {
                    // send the PeerId first
                    if (local_stream.write_all(peer_id.to_string().as_bytes()).await).is_err() {
                        println!("Failed to write peer id to ipc stream");
                        return;
                    };
                    tokio::io::copy(&mut libp2p_stream.compat(), &mut local_stream)
                        .await
                        .ok();
                });
            }
        });
        streams_map_lock.insert(protocol, task);
        Ok(())
    }

    async fn close_accepting_stream(self, _: Context, protocol: String) -> Result<(), String> {
        let protocol = StreamProtocol::try_from_owned(protocol).map_err(|e| e.to_string())?;
        let mut streams_map_lock = self.accept_streams.lock().unwrap();
        if let Some(task) = streams_map_lock.remove(&protocol) {
            task.abort();
        }
        Ok(())
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
