use libp2p_swarm::StreamProtocol;

pub const FUNGI_REMOTE_ACCESS_PROTOCOL: StreamProtocol =
    StreamProtocol::new("/fungi/remote-access/0.1.0");
pub const FUNGI_PROBE_PROTOCOL: StreamProtocol = StreamProtocol::new("/fungi/probe/0.1.0");
pub const FUNGI_RELAY_HANDSHAKE_PROTOCOL: StreamProtocol =
    StreamProtocol::new("/fungi/relay-handshake/0.1.0");
pub const FUNGI_RELAY_REFRESH_PROTOCOL: StreamProtocol =
    StreamProtocol::new("/fungi/relay-refresh/0.1.0");
pub const FUNGI_FILE_TRANSFER_PROTOCOL: StreamProtocol =
    StreamProtocol::new("/fungi/file-transfer/0.1.0");
pub const FUNGI_PEER_HANDSHAKE_PROTOCOL: StreamProtocol =
    StreamProtocol::new("/fungi/peer-handshake/0.1.0");
pub const FUNGI_SERVICE_DISCOVERY_PROTOCOL: StreamProtocol =
    StreamProtocol::new("/fungi/services/0.1.0");
pub const FUNGI_NODE_CAPABILITIES_PROTOCOL: StreamProtocol =
    StreamProtocol::new("/fungi/node-capabilities/0.1.0");
pub const FUNGI_SERVICE_CONTROL_PROTOCOL: StreamProtocol =
    StreamProtocol::new("/fungi/service-control/0.1.0");

pub const FUNGI_TUNNEL_PROTOCOL: &str = "/fungi/tunnel/0.1.0";
pub const FUNGI_SERVICE_PORT_PROTOCOL_PREFIX: &str = "/fungi/service-port";

pub fn service_port_protocol(service_id: &str, port_name: &str) -> String {
    format!(
        "{}/{}/{}/0.1.0",
        FUNGI_SERVICE_PORT_PROTOCOL_PREFIX,
        protocol_component(service_id),
        protocol_component(port_name),
    )
}

fn protocol_component(value: &str) -> String {
    let normalized = value
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();

    if normalized.is_empty() {
        "default".to_string()
    } else {
        normalized
    }
}
