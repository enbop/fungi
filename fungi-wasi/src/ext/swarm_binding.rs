use super::{
    fungi::ext::swarm::{self, Pollable},
    wasi::io::streams::{InputStream, OutputStream},
};
use fungi_daemon::listeners::FungiDaemonRpcClient;
use interprocess::local_socket::tokio::Listener;
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use wasmtime::component::Resource;
use wasmtime_wasi::ResourceTable;

pub struct SwarmBinding {
    table: ResourceTable,
    ipc_dir: PathBuf,
    daemon_rpc_client: Option<FungiDaemonRpcClient>,
    accept_streams: Arc<Mutex<HashSet<String>>>,
}

impl SwarmBinding {
    pub fn new(daemon_rpc_client: Option<FungiDaemonRpcClient>, ipc_dir: PathBuf) -> Self {
        Self {
            ipc_dir,
            daemon_rpc_client,
            table: Default::default(),
            accept_streams: Default::default(),
        }
    }
}

pub struct IncomingStreams {
    protocol: String,
    listener: Listener,
}

#[async_trait::async_trait]
impl swarm::HostIncomingStreams for SwarmBinding {
    async fn next(
        &mut self,
        this: Resource<IncomingStreams>,
    ) -> Result<(String, Resource<InputStream>, Resource<OutputStream>), swarm::Error> {
        todo!()
    }

    fn subscribe(&mut self, this: Resource<IncomingStreams>) -> Resource<Pollable> {
        todo!()
    }

    async fn drop(&mut self, this: Resource<IncomingStreams>) -> Result<(), anyhow::Error> {
        if let Ok(incoming_streams) = self.table.delete(this) {
            self.daemon_rpc_client
                .as_ref()
                .unwrap()
                .close_accepting_stream(
                    tarpc::context::current(),
                    incoming_streams.protocol.clone(),
                )
                .await
                .ok();

            self.accept_streams
                .lock()
                .unwrap()
                .remove(&incoming_streams.protocol);
        };

        Ok(())
    }
}

#[async_trait::async_trait]
impl swarm::Host for SwarmBinding {
    async fn peer_id(&mut self) -> Result<String, swarm::Error> {
        let Some(daemon_rpc_client) = self.daemon_rpc_client.as_ref() else {
            return Err(swarm::Error::DaemonNotAvailable);
        };
        daemon_rpc_client
            .peer_id(tarpc::context::current())
            .await
            .map(|peer_id| peer_id.to_string())
            .map_err(|_| swarm::Error::RpcError)
    }

    async fn accept_stream(
        &mut self,
        protocol: String,
    ) -> Result<Resource<IncomingStreams>, swarm::Error> {
        let Some(daemon_rpc_client) = self.daemon_rpc_client.as_ref() else {
            return Err(swarm::Error::DaemonNotAvailable);
        };

        if self.accept_streams.lock().unwrap().contains(&protocol) {
            return Err(swarm::Error::StreamAlreadyExists);
        }

        let ipc_path = self
            .ipc_dir
            .join(format!("fungi-ext-accept-stream-{}.sock", protocol));
        let ipc_sock_name = ipc_path.to_string_lossy().to_string();

        let listener: Listener =
            fungi_util::ipc::create_ipc_listener(&ipc_sock_name).map_err(|e| {
                println!("Failed to create ipc listener: {}", e);
                swarm::Error::IpcError
            })?;

        if let Err(_) = daemon_rpc_client
            .accept_stream(tarpc::context::current(), protocol.clone(), ipc_sock_name)
            .await
        {
            return Err(swarm::Error::RpcError);
        };

        let incoming_streams = IncomingStreams {
            protocol: protocol.clone(),
            listener,
        };
        let r = self.table.push(incoming_streams).unwrap(); // TODO unwrap

        let mut accept_streams_guard = self.accept_streams.lock().unwrap();
        if accept_streams_guard.contains(&protocol) {
            return Err(swarm::Error::StreamAlreadyExists);
        }
        accept_streams_guard.insert(protocol);

        Ok(r)
    }
}
