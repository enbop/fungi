use libp2p_swarm::StreamProtocol;

pub const FUNGI_REMOTE_ACCESS_PROTOCOL: StreamProtocol =
    StreamProtocol::new("/fungi/remote-access/0.1.0");
pub const FUNGI_RELAY_HANDSHAKE_PROTOCOL: StreamProtocol =
    StreamProtocol::new("/fungi/relay-handshake/0.1.0");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_component_empty_string_returns_default() {
        assert_eq!(protocol_component(""), "default");
    }

    #[test]
    fn protocol_component_whitespace_only_returns_default() {
        assert_eq!(protocol_component("   "), "default");
    }

    #[test]
    fn protocol_component_lowercases_ascii() {
        assert_eq!(protocol_component("MyService"), "myservice");
        assert_eq!(protocol_component("HTTP"), "http");
    }

    #[test]
    fn protocol_component_replaces_spaces_with_dash() {
        assert_eq!(protocol_component("my service"), "my-service");
    }

    #[test]
    fn protocol_component_replaces_special_chars_with_dash() {
        assert_eq!(protocol_component("my@service!"), "my-service-");
        assert_eq!(protocol_component("a/b"), "a-b");
    }

    #[test]
    fn protocol_component_preserves_allowed_chars() {
        assert_eq!(protocol_component("my-service_v1.0"), "my-service_v1.0");
    }

    #[test]
    fn protocol_component_trims_surrounding_whitespace() {
        assert_eq!(protocol_component("  hello  "), "hello");
    }

    #[test]
    fn service_port_protocol_formats_correctly() {
        let result = service_port_protocol("my-service", "http");
        assert_eq!(result, "/fungi/service-port/my-service/http/0.1.0");
    }

    #[test]
    fn service_port_protocol_normalizes_components() {
        let result = service_port_protocol("My Service", "HTTP Port");
        assert_eq!(result, "/fungi/service-port/my-service/http-port/0.1.0");
    }

    #[test]
    fn service_port_protocol_uses_default_for_empty_parts() {
        let result = service_port_protocol("", "");
        assert_eq!(result, "/fungi/service-port/default/default/0.1.0");
    }

    #[test]
    fn service_port_protocol_starts_with_prefix() {
        let result = service_port_protocol("svc", "port");
        assert!(result.starts_with(FUNGI_SERVICE_PORT_PROTOCOL_PREFIX));
    }

    #[test]
    fn protocol_constants_have_expected_values() {
        assert_eq!(FUNGI_TUNNEL_PROTOCOL, "/fungi/tunnel/0.1.0");
        assert_eq!(FUNGI_SERVICE_PORT_PROTOCOL_PREFIX, "/fungi/service-port");
    }
}
