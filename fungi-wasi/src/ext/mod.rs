mod swarm_binding;
use fungi_daemon::listeners::FungiDaemonRpcClient;
use swarm_binding::SwarmBinding;
use wasmtime::component::bindgen;

bindgen!({
    path: "./wit",
    world: "bindings",
    async: true,
    with: {
        "fungi:ext/swarm/stream-control": swarm_binding::StreamControl,
    }
});

pub struct FungiExt {
    pub swarm: SwarmBinding,
}

impl FungiExt {
    pub fn new(daemon_rpc_client: Option<FungiDaemonRpcClient>) -> Self {
        Self {
            swarm: SwarmBinding::new(daemon_rpc_client),
        }
    }
}
