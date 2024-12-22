use super::fungi::ext::swarm;
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

pub struct StreamControl {}

// impl StreamControl {
//     fn accept(protocol: String) -> Result<String, String> {
//         todo!()
//     }
// }

#[async_trait::async_trait]
impl swarm::HostStreamControl for SwarmBinding {
    async fn drop(&mut self, this: Resource<StreamControl>) -> Result<(), anyhow::Error> {
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

    async fn create_stream_control(&mut self) -> Resource<StreamControl> {
        todo!()
    }
}
