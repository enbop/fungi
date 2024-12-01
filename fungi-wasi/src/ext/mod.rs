mod swarm_binding;
use swarm_binding::SwarmBinding;
use wasmtime::component::bindgen;

bindgen!("bindings" in "./wit");

pub struct FungiExt {
    pub swarm: SwarmBinding,
}

impl FungiExt {
    pub fn new() -> Self {
        Self {
            swarm: SwarmBinding {},
        }
    }
}
