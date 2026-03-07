use fungi_config::FungiConfig;
use serde::{Deserialize, Serialize};

use crate::RuntimeControl;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeCapabilities {
    pub runtimes: NodeRuntimeCapabilities,
    pub allowed_tcp_ports: NodeAllowedTcpPorts,
    pub storage_roots: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeRuntimeCapabilities {
    pub docker: bool,
    pub wasmtime: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeAllowedTcpPorts {
    pub ports: Vec<u16>,
    pub ranges: Vec<NodePortRange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodePortRange {
    pub start: u16,
    pub end: u16,
}

pub fn build_local_node_capabilities(
    config: &FungiConfig,
    runtime_control: &RuntimeControl,
) -> NodeCapabilities {
    let mut allowed_ports = config.docker.allowed_ports.clone();
    allowed_ports.sort_unstable();
    allowed_ports.dedup();

    let mut ranges = config
        .docker
        .allowed_port_ranges
        .iter()
        .map(|range| NodePortRange {
            start: range.start,
            end: range.end,
        })
        .collect::<Vec<_>>();
    ranges.sort_by(|left, right| {
        left.start
            .cmp(&right.start)
            .then(left.end.cmp(&right.end))
    });

    NodeCapabilities {
        runtimes: NodeRuntimeCapabilities {
            docker: runtime_control.supports(crate::RuntimeKind::Docker),
            wasmtime: runtime_control.supports(crate::RuntimeKind::Wasmtime),
        },
        allowed_tcp_ports: NodeAllowedTcpPorts {
            ports: allowed_ports,
            ranges,
        },
        storage_roots: vec!["fungi_home".to_string()],
    }
}