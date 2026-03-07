use crate::{
    AgentPolicy, DockerAgentError, Result,
    client::{
        CreateContainerBody, CreateContainerRequest, DockerClient, HostConfig, HostPortBinding,
        InspectContainerResponse,
    },
    spec::{ContainerSpec, LogsOptions, PortProtocol},
};
use serde::Serialize;
use std::collections::BTreeMap;

pub struct DockerAgent {
    policy: AgentPolicy,
    client: DockerClient,
}

impl DockerAgent {
    pub fn new(policy: AgentPolicy) -> Self {
        let client = DockerClient::new(&policy.socket_path);
        Self { policy, client }
    }

    pub async fn create_container(&self, spec: &ContainerSpec) -> Result<ContainerDetails> {
        self.policy.validate_create_spec(spec)?;

        let request = CreateContainerRequest {
            name: spec.name.clone(),
            body: to_create_body(spec, &self.policy),
        };
        let created = self.client.create_container(&request).await?;
        self.inspect_container(&created.id).await
    }

    pub async fn start_container(&self, id: &str) -> Result<()> {
        self.ensure_managed(id).await?;
        self.client.start_container(id).await
    }

    pub async fn stop_container(&self, id: &str) -> Result<()> {
        self.ensure_managed(id).await?;
        self.client.stop_container(id).await
    }

    pub async fn remove_container(&self, id: &str) -> Result<()> {
        self.ensure_managed(id).await?;
        self.client.remove_container(id).await
    }

    pub async fn inspect_container(&self, id: &str) -> Result<ContainerDetails> {
        let details = self.client.inspect_container(id).await?;
        ensure_managed_labels(&self.policy, &details)?;
        Ok(map_container_details(details))
    }

    pub async fn container_logs(&self, id: &str, options: &LogsOptions) -> Result<ContainerLogs> {
        self.ensure_managed(id).await?;
        let raw = self.client.container_logs(id, options).await?;
        Ok(ContainerLogs {
            text: decode_log_frames(&raw),
            raw,
        })
    }

    async fn ensure_managed(&self, id: &str) -> Result<()> {
        let details = self.client.inspect_container(id).await?;
        ensure_managed_labels(&self.policy, &details)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ContainerDetails {
    pub id: String,
    pub name: String,
    pub image: String,
    pub labels: BTreeMap<String, String>,
    pub state: ContainerState,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContainerState {
    pub status: String,
    pub running: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContainerLogs {
    pub raw: Vec<u8>,
    pub text: String,
}

fn ensure_managed_labels(policy: &AgentPolicy, details: &InspectContainerResponse) -> Result<()> {
    let (label_key, label_value) = policy.managed_label();
    let actual = details.config.labels.get(label_key);
    if actual.map(String::as_str) == Some(label_value) {
        return Ok(());
    }

    Err(DockerAgentError::PolicyDenied(format!(
        "container is not managed by fungi: {}",
        details.id
    )))
}

fn map_container_details(details: InspectContainerResponse) -> ContainerDetails {
    ContainerDetails {
        id: details.id,
        name: details.name.trim_start_matches('/').to_string(),
        image: details.config.image,
        labels: details.config.labels,
        state: ContainerState {
            status: details.state.status,
            running: details.state.running,
        },
    }
}

fn to_create_body(spec: &ContainerSpec, policy: &AgentPolicy) -> CreateContainerBody {
    let mut labels = spec.labels.clone();
    labels.insert(
        policy.managed_label_key.clone(),
        policy.managed_label_value.clone(),
    );

    let env = spec.env.iter().map(|(key, value)| format!("{key}={value}")).collect();
    let binds = spec
        .mounts
        .iter()
        .map(|mount| format!("{}:{}", mount.host_path.display(), mount.container_path))
        .collect();

    let mut exposed_ports = BTreeMap::new();
    let mut port_bindings = BTreeMap::new();
    for port in &spec.ports {
        let key = format!("{}/{}", port.container_port, protocol_name(port.protocol));
        exposed_ports.insert(key.clone(), BTreeMap::new());
        port_bindings.insert(
            key,
            vec![HostPortBinding {
                host_port: port.host_port.to_string(),
            }],
        );
    }

    CreateContainerBody {
        image: spec.image.clone(),
        env,
        cmd: spec.command.clone(),
        entrypoint: spec.entrypoint.clone(),
        working_dir: spec.working_dir.clone(),
        labels,
        exposed_ports,
        host_config: HostConfig { binds, port_bindings },
    }
}

fn protocol_name(protocol: PortProtocol) -> &'static str {
    match protocol {
        PortProtocol::Tcp => "tcp",
        PortProtocol::Udp => "udp",
    }
}

fn decode_log_frames(bytes: &[u8]) -> String {
    let mut output = String::new();
    let mut offset = 0;
    let mut parsed_frame = false;

    while bytes.len().saturating_sub(offset) >= 8 {
        let frame_len = u32::from_be_bytes([
            bytes[offset + 4],
            bytes[offset + 5],
            bytes[offset + 6],
            bytes[offset + 7],
        ]) as usize;
        let frame_start = offset + 8;
        let frame_end = frame_start + frame_len;
        if frame_end > bytes.len() {
            break;
        }
        output.push_str(&String::from_utf8_lossy(&bytes[frame_start..frame_end]));
        offset = frame_end;
        parsed_frame = true;
    }

    if parsed_frame && offset == bytes.len() {
        output
    } else {
        String::from_utf8_lossy(bytes).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_plain_logs() {
        assert_eq!(decode_log_frames(b"hello\n"), "hello\n");
    }

    #[test]
    fn decodes_multiplexed_logs() {
        let payload = [
            1, 0, 0, 0, 0, 0, 0, 6, b'h', b'e', b'l', b'l', b'o', b'\n',
        ];
        assert_eq!(decode_log_frames(&payload), "hello\n");
    }
}