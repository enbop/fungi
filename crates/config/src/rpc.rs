use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Rpc {
    #[serde(default = "default_rpc_listen_address")]
    pub listen_address: String,
}

impl Default for Rpc {
    fn default() -> Self {
        Self {
            listen_address: default_rpc_listen_address(),
        }
    }
}

fn default_rpc_listen_address() -> String {
    crate::DEFAULT_RPC_ADDRESS.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_rpc_listen_address_is_loopback() {
        let rpc = Rpc::default();
        assert_eq!(rpc.listen_address, "127.0.0.1:5405");
    }

    #[test]
    fn rpc_deserializes_default_when_field_missing() {
        let toml = "[rpc]\n";
        let config: toml::Value = toml::from_str(toml).unwrap();
        let rpc: Rpc = config["rpc"].clone().try_into().unwrap();
        assert_eq!(rpc.listen_address, "127.0.0.1:5405");
    }

    #[test]
    fn rpc_deserializes_custom_listen_address() {
        let toml = "[rpc]\nlisten_address = \"0.0.0.0:9000\"\n";
        let config: toml::Value = toml::from_str(toml).unwrap();
        let rpc: Rpc = config["rpc"].clone().try_into().unwrap();
        assert_eq!(rpc.listen_address, "0.0.0.0:9000");
    }

    #[test]
    fn rpc_clone_is_independent() {
        let rpc = Rpc::default();
        let mut cloned = rpc.clone();
        cloned.listen_address = "0.0.0.0:8080".to_string();
        assert_eq!(rpc.listen_address, "127.0.0.1:5405");
    }
}
