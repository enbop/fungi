mod swarm_binding;
use fungi_daemon::listeners::FungiDaemonRpcClient;
use swarm_binding::SwarmBinding;
use wasmtime::component::bindgen;

bindgen!({
    path: "./wit",
    world: "bindings",
    async: {
        only_imports: [
            "peer-id",
            "accept-stream",
            "[method]incoming-streams.next",
        ]
    },
    with: {
        "fungi:ext/swarm/incoming-streams": swarm_binding::IncomingStreams,
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
