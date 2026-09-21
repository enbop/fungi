use fungi_config::FungiConfig;
use serde::{Deserialize, Serialize};

use crate::runtime::wasmtime_runtime_supported;
use crate::{RuntimeControl, RuntimeKind};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeCapabilities {
    pub runtimes: NodeRuntimeCapabilities,
    pub storage_roots: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeRuntimeCapabilities {
    pub wasmtime: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalRuntimeStatus {
    pub wasmtime: LocalRuntimeAvailability,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalRuntimeAvailability {
    pub config_enabled: bool,
    pub detected: bool,
    pub active: bool,
    pub endpoint: Option<String>,
}

pub fn build_local_node_capabilities(
    _config: &FungiConfig,
    runtime_control: &RuntimeControl,
) -> NodeCapabilities {
    NodeCapabilities {
        runtimes: NodeRuntimeCapabilities {
            wasmtime: runtime_control.supports(crate::RuntimeKind::Wasmtime),
        },
        storage_roots: vec!["fungi_home".to_string()],
    }
}

pub fn build_local_runtime_status(
    config: &FungiConfig,
    runtime_control: &RuntimeControl,
) -> LocalRuntimeStatus {
    LocalRuntimeStatus {
        wasmtime: LocalRuntimeAvailability {
            config_enabled: !config.runtime.disable_wasmtime,
            detected: wasmtime_runtime_supported(),
            active: runtime_control.supports(RuntimeKind::Wasmtime),
            endpoint: None,
        },
    }
}
