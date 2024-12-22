use super::{
    fungi::ext::swarm::{self, Pollable},
    wasi::io::streams::{InputStream, OutputStream},
};
use fungi_daemon::listeners::FungiDaemonRpcClient;
use wasmtime::component::Resource;

pub struct SwarmBinding {
    daemon_rpc_client: Option<FungiDaemonRpcClient>,
}

impl SwarmBinding {
    pub fn new(daemon_rpc_client: Option<FungiDaemonRpcClient>) -> Self {
        Self { daemon_rpc_client }
    }
}

pub struct IncomingStreams {}

#[async_trait::async_trait]
impl swarm::HostIncomingStreams for SwarmBinding {
    async fn next(
        &mut self,
        this: Resource<IncomingStreams>,
    ) -> Result<(String, Resource<InputStream>, Resource<OutputStream>), String> {
        todo!()
    }

    fn subscribe(&mut self, this: Resource<IncomingStreams>) -> Resource<Pollable> {
        todo!()
    }

    fn drop(&mut self, this: Resource<IncomingStreams>) -> Result<(), anyhow::Error> {
        drop(this);
        Ok(())
    }
}

#[async_trait::async_trait]
impl swarm::Host for SwarmBinding {
    async fn peer_id(&mut self) -> String {
        let Some(daemon_rpc_client) = self.daemon_rpc_client.as_ref() else {
            return "Daemon RPC not available".to_string();
        };
        match daemon_rpc_client.peer_id(tarpc::context::current()).await {
            Ok(peer_id) => peer_id.to_string(),
            Err(e) => format!("Failed to get peer id: {}", e),
        }
    }

    async fn accept_stream(&mut self, protocol: String) -> Resource<IncomingStreams> {
        todo!()
    }
}
