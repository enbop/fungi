use std::net::{AddrParseError, SocketAddr};

use serde::{Deserialize, Serialize};

impl TryInto<SocketAddr> for &ListeningRule {
    type Error = AddrParseError;

    fn try_into(self) -> Result<SocketAddr, Self::Error> {
        format!("{}:{}", self.host, self.port).parse()
    }
}

impl TryInto<SocketAddr> for &ForwardingRule {
    type Error = AddrParseError;

    fn try_into(self) -> Result<SocketAddr, Self::Error> {
        format!("{}:{}", self.local_host, self.local_port).parse()
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ForwardingRule {
    pub local_host: String,
    pub local_port: u16,

    pub remote_peer_id: String,
    #[serde(default)]
    pub remote_protocol: Option<String>,
    pub remote_port: u16,
    #[serde(default)]
    pub remote_service_id: Option<String>,
    #[serde(default)]
    pub remote_service_name: Option<String>,
    #[serde(default)]
    pub remote_service_port_name: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ListeningRule {
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub protocol: Option<String>,
    // #[serde(default)]
    // pub allowed_peers: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Forwarding {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub rules: Vec<ForwardingRule>,
}

impl Default for Forwarding {
    fn default() -> Self {
        Self {
            enabled: true,
            rules: Default::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Listening {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub rules: Vec<ListeningRule>,
}

impl Default for Listening {
    fn default() -> Self {
        Self {
            enabled: true,
            rules: Default::default(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct TcpTunneling {
    #[serde(default)]
    pub forwarding: Forwarding,
    #[serde(default)]
    pub listening: Listening,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;

    // --- Default implementations ---

    #[test]
    fn forwarding_default_is_enabled_with_no_rules() {
        let f = Forwarding::default();
        assert!(f.enabled);
        assert!(f.rules.is_empty());
    }

    #[test]
    fn listening_default_is_enabled_with_no_rules() {
        let l = Listening::default();
        assert!(l.enabled);
        assert!(l.rules.is_empty());
    }

    #[test]
    fn tcp_tunneling_default_has_enabled_forwarding_and_listening() {
        let t = TcpTunneling::default();
        assert!(t.forwarding.enabled);
        assert!(t.listening.enabled);
    }

    // --- TryInto<SocketAddr> for ListeningRule ---

    #[test]
    fn listening_rule_converts_to_socket_addr_ipv4() {
        let rule = ListeningRule {
            host: "127.0.0.1".to_string(),
            port: 8080,
            protocol: None,
        };
        let addr: SocketAddr = (&rule).try_into().unwrap();
        assert_eq!(addr.to_string(), "127.0.0.1:8080");
    }

    #[test]
    fn listening_rule_converts_to_socket_addr_port_zero() {
        let rule = ListeningRule {
            host: "0.0.0.0".to_string(),
            port: 0,
            protocol: None,
        };
        let addr: SocketAddr = (&rule).try_into().unwrap();
        assert_eq!(addr.port(), 0);
        assert_eq!(addr.ip().to_string(), "0.0.0.0");
    }

    #[test]
    fn listening_rule_invalid_host_returns_error() {
        let rule = ListeningRule {
            host: "not-an-ip".to_string(),
            port: 80,
            protocol: None,
        };
        let result: Result<SocketAddr, _> = (&rule).try_into();
        assert!(result.is_err());
    }

    // --- TryInto<SocketAddr> for ForwardingRule ---

    #[test]
    fn forwarding_rule_converts_to_socket_addr() {
        let rule = ForwardingRule {
            local_host: "0.0.0.0".to_string(),
            local_port: 9090,
            remote_peer_id: "peer123".to_string(),
            remote_port: 80,
            ..Default::default()
        };
        let addr: SocketAddr = (&rule).try_into().unwrap();
        assert_eq!(addr.to_string(), "0.0.0.0:9090");
    }

    #[test]
    fn forwarding_rule_invalid_local_host_returns_error() {
        let rule = ForwardingRule {
            local_host: "bad-host".to_string(),
            local_port: 1234,
            ..Default::default()
        };
        let result: Result<SocketAddr, _> = (&rule).try_into();
        assert!(result.is_err());
    }

    // --- TOML deserialization ---

    #[test]
    fn tcp_tunneling_deserializes_from_empty_toml() {
        let toml = "[tcp_tunneling]\n";
        let config: toml::Value = toml::from_str(toml).unwrap();
        let t: TcpTunneling = config["tcp_tunneling"].clone().try_into().unwrap();
        assert!(t.forwarding.rules.is_empty());
        assert!(t.listening.rules.is_empty());
    }

    #[test]
    fn forwarding_rule_deserializes_optional_fields_as_none() {
        let toml = r#"
            local_host = "127.0.0.1"
            local_port = 3000
            remote_peer_id = "peer1"
            remote_port = 4000
        "#;
        let rule: ForwardingRule = toml::from_str(toml).unwrap();
        assert!(rule.remote_protocol.is_none());
        assert!(rule.remote_service_id.is_none());
        assert!(rule.remote_service_name.is_none());
        assert!(rule.remote_service_port_name.is_none());
    }

    #[test]
    fn listening_rule_deserializes_optional_protocol_as_none() {
        let toml = "host = \"127.0.0.1\"\nport = 8080\n";
        let rule: ListeningRule = toml::from_str(toml).unwrap();
        assert!(rule.protocol.is_none());
    }

    #[test]
    fn listening_rule_deserializes_with_explicit_protocol() {
        let toml = "host = \"127.0.0.1\"\nport = 8080\nprotocol = \"/fungi/tunnel/0.1.0\"\n";
        let rule: ListeningRule = toml::from_str(toml).unwrap();
        assert_eq!(rule.protocol, Some("/fungi/tunnel/0.1.0".to_string()));
    }
}
