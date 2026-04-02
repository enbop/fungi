use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContainerSpec {
    pub name: Option<String>,
    pub image: String,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub mounts: Vec<BindMount>,
    #[serde(default)]
    pub ports: Vec<PortBinding>,
    #[serde(default)]
    pub command: Vec<String>,
    #[serde(default)]
    pub entrypoint: Vec<String>,
    pub working_dir: Option<String>,
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindMount {
    pub host_path: PathBuf,
    pub container_path: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum PortProtocol {
    #[default]
    Tcp,
    Udp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortBinding {
    pub host_port: u16,
    pub container_port: u16,
    #[serde(default)]
    pub protocol: PortProtocol,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogsOptions {
    #[serde(default = "default_true")]
    pub stdout: bool,
    #[serde(default = "default_true")]
    pub stderr: bool,
    pub tail: Option<String>,
}

impl Default for LogsOptions {
    fn default() -> Self {
        Self {
            stdout: true,
            stderr: true,
            tail: None,
        }
    }
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_spec_default_has_empty_image_and_no_name() {
        let spec = ContainerSpec::default();
        assert_eq!(spec.image, "");
        assert!(spec.name.is_none());
        assert!(spec.working_dir.is_none());
        assert!(spec.env.is_empty());
        assert!(spec.mounts.is_empty());
        assert!(spec.ports.is_empty());
        assert!(spec.command.is_empty());
        assert!(spec.entrypoint.is_empty());
        assert!(spec.labels.is_empty());
    }

    #[test]
    fn port_protocol_default_is_tcp() {
        assert_eq!(PortProtocol::default(), PortProtocol::Tcp);
    }

    #[test]
    fn port_protocol_serializes_as_lowercase() {
        let tcp = serde_json::to_string(&PortProtocol::Tcp).unwrap();
        let udp = serde_json::to_string(&PortProtocol::Udp).unwrap();
        assert_eq!(tcp, "\"tcp\"");
        assert_eq!(udp, "\"udp\"");
    }

    #[test]
    fn port_protocol_deserializes_from_lowercase() {
        let tcp: PortProtocol = serde_json::from_str("\"tcp\"").unwrap();
        let udp: PortProtocol = serde_json::from_str("\"udp\"").unwrap();
        assert_eq!(tcp, PortProtocol::Tcp);
        assert_eq!(udp, PortProtocol::Udp);
    }

    #[test]
    fn logs_options_default_captures_both_streams_without_tail() {
        let opts = LogsOptions::default();
        assert!(opts.stdout);
        assert!(opts.stderr);
        assert!(opts.tail.is_none());
    }

    #[test]
    fn logs_options_serializes_and_deserializes() {
        let opts = LogsOptions {
            stdout: true,
            stderr: false,
            tail: Some("100".to_string()),
        };
        let json = serde_json::to_string(&opts).unwrap();
        let decoded: LogsOptions = serde_json::from_str(&json).unwrap();
        assert!(decoded.stdout);
        assert!(!decoded.stderr);
        assert_eq!(decoded.tail, Some("100".to_string()));
    }

    #[test]
    fn container_spec_serializes_and_deserializes_round_trip() {
        let mut spec = ContainerSpec::default();
        spec.image = "ubuntu:latest".to_string();
        spec.name = Some("my-container".to_string());
        spec.env.insert("MY_VAR".to_string(), "value".to_string());
        spec.command = vec!["bash".to_string(), "-c".to_string(), "echo hi".to_string()];

        let json = serde_json::to_string(&spec).unwrap();
        let decoded: ContainerSpec = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.image, "ubuntu:latest");
        assert_eq!(decoded.name, Some("my-container".to_string()));
        assert_eq!(decoded.env.get("MY_VAR"), Some(&"value".to_string()));
        assert_eq!(decoded.command, vec!["bash", "-c", "echo hi"]);
    }

    #[test]
    fn bind_mount_serializes_and_deserializes() {
        let mount = BindMount {
            host_path: PathBuf::from("/host/data"),
            container_path: "/container/data".to_string(),
        };
        let json = serde_json::to_string(&mount).unwrap();
        let decoded: BindMount = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.host_path, PathBuf::from("/host/data"));
        assert_eq!(decoded.container_path, "/container/data");
    }

    #[test]
    fn port_binding_defaults_to_tcp_protocol() {
        let json = r#"{"host_port": 8080, "container_port": 80}"#;
        let binding: PortBinding = serde_json::from_str(json).unwrap();
        assert_eq!(binding.host_port, 8080);
        assert_eq!(binding.container_port, 80);
        assert_eq!(binding.protocol, PortProtocol::Tcp);
    }

    #[test]
    fn container_spec_env_is_ordered_by_key() {
        let mut spec = ContainerSpec::default();
        spec.env.insert("Z_VAR".to_string(), "z".to_string());
        spec.env.insert("A_VAR".to_string(), "a".to_string());
        spec.env.insert("M_VAR".to_string(), "m".to_string());

        let keys: Vec<&String> = spec.env.keys().collect();
        assert_eq!(keys, vec!["A_VAR", "M_VAR", "Z_VAR"]);
    }
}
