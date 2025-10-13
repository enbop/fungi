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
    "127.0.0.1:54000".to_string()
}
