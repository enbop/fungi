use crate::{DockerAgentError, Result, spec::LogsOptions};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::{Method, Request, StatusCode, client::conn::http1, header};
use hyper_util::rt::TokioIo;
use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{collections::BTreeMap, path::Path};
use tokio::net::UnixStream;

const QUERY_ENCODE_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'&')
    .add(b'+')
    .add(b'?');

pub struct DockerClient {
    socket_path: std::path::PathBuf,
}

impl DockerClient {
    pub fn new(socket_path: impl AsRef<Path>) -> Self {
        Self {
            socket_path: socket_path.as_ref().to_path_buf(),
        }
    }

    pub async fn create_container(
        &self,
        request: &CreateContainerRequest,
    ) -> Result<CreateContainerResponse> {
        let mut path = String::from("/containers/create");
        if let Some(name) = &request.name {
            path.push_str("?name=");
            path.push_str(&utf8_percent_encode(name, QUERY_ENCODE_SET).to_string());
        }

        self.send_json(Method::POST, &path, Some(&request.body))
            .await
    }

    pub async fn start_container(&self, id: &str) -> Result<()> {
        let path = format!("/containers/{id}/start");
        self.send_empty(Method::POST, &path).await
    }

    pub async fn stop_container(&self, id: &str) -> Result<()> {
        let path = format!("/containers/{id}/stop");
        self.send_empty(Method::POST, &path).await
    }

    pub async fn remove_container(&self, id: &str) -> Result<()> {
        let path = format!("/containers/{id}");
        self.send_empty(Method::DELETE, &path).await
    }

    pub async fn inspect_container(&self, id: &str) -> Result<InspectContainerResponse> {
        let path = format!("/containers/{id}/json");
        self.send_json(Method::GET, &path, Option::<&()>::None)
            .await
    }

    pub async fn container_logs(&self, id: &str, options: &LogsOptions) -> Result<Vec<u8>> {
        let mut path = format!(
            "/containers/{id}/logs?stdout={}&stderr={}",
            options.stdout, options.stderr
        );
        if let Some(tail) = &options.tail {
            path.push_str("&tail=");
            path.push_str(&utf8_percent_encode(tail, QUERY_ENCODE_SET).to_string());
        } else {
            path.push_str("&tail=all");
        }

        self.send_bytes(Method::GET, &path).await
    }

    async fn send_empty(&self, method: Method, path: &str) -> Result<()> {
        let response = self.send(method, path, Vec::new(), None).await?;
        if response.status != StatusCode::NO_CONTENT && response.status != StatusCode::NOT_MODIFIED
        {
            return Err(api_error(response.status, &response.body)?);
        }
        Ok(())
    }

    async fn send_json<TReq, TResp>(
        &self,
        method: Method,
        path: &str,
        body: Option<&TReq>,
    ) -> Result<TResp>
    where
        TReq: Serialize,
        TResp: DeserializeOwned,
    {
        let bytes = if let Some(value) = body {
            serde_json::to_vec(value)?
        } else {
            Vec::new()
        };
        let content_type = if bytes.is_empty() {
            None
        } else {
            Some("application/json")
        };
        let response = self.send(method, path, bytes, content_type).await?;
        if !response.status.is_success() {
            return Err(api_error(response.status, &response.body)?);
        }
        Ok(serde_json::from_slice(&response.body)?)
    }

    async fn send_bytes(&self, method: Method, path: &str) -> Result<Vec<u8>> {
        let response = self.send(method, path, Vec::new(), None).await?;
        if !response.status.is_success() {
            return Err(api_error(response.status, &response.body)?);
        }
        Ok(response.body)
    }

    async fn send(
        &self,
        method: Method,
        path: &str,
        body: Vec<u8>,
        content_type: Option<&str>,
    ) -> Result<HttpResponse> {
        let stream = UnixStream::connect(&self.socket_path).await?;
        let io = TokioIo::new(stream);
        let (mut sender, connection) = http1::handshake(io).await?;
        tokio::spawn(async move {
            let _ = connection.await;
        });

        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header(header::HOST, "docker")
            .header(header::CONTENT_LENGTH, body.len());
        if let Some(content_type) = content_type {
            request = request.header(header::CONTENT_TYPE, content_type);
        }

        let response = sender
            .send_request(request.body(Full::new(Bytes::from(body)))?)
            .await?;
        let status = response.status();
        let body = response.into_body().collect().await?.to_bytes().to_vec();
        Ok(HttpResponse { status, body })
    }
}

fn api_error(status: StatusCode, body: &[u8]) -> Result<DockerAgentError> {
    #[derive(Deserialize)]
    struct ErrorMessage {
        message: String,
    }

    let message = serde_json::from_slice::<ErrorMessage>(body)
        .map(|value| value.message)
        .unwrap_or_else(|_| String::from_utf8_lossy(body).trim().to_string());
    Ok(DockerAgentError::DockerApi { status, message })
}

struct HttpResponse {
    status: StatusCode,
    body: Vec<u8>,
}

#[derive(Debug, Serialize)]
pub struct CreateContainerRequest {
    pub name: Option<String>,
    #[serde(flatten)]
    pub body: CreateContainerBody,
}

#[derive(Debug, Serialize)]
pub struct CreateContainerBody {
    #[serde(rename = "Image")]
    pub image: String,
    #[serde(rename = "Env", skip_serializing_if = "Vec::is_empty")]
    pub env: Vec<String>,
    #[serde(rename = "Cmd", skip_serializing_if = "Vec::is_empty")]
    pub cmd: Vec<String>,
    #[serde(rename = "Entrypoint", skip_serializing_if = "Vec::is_empty")]
    pub entrypoint: Vec<String>,
    #[serde(rename = "WorkingDir", skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
    #[serde(rename = "Labels")]
    pub labels: BTreeMap<String, String>,
    #[serde(rename = "ExposedPorts", skip_serializing_if = "BTreeMap::is_empty")]
    pub exposed_ports: BTreeMap<String, BTreeMap<String, String>>,
    #[serde(rename = "HostConfig")]
    pub host_config: HostConfig,
}

#[derive(Debug, Serialize, Default)]
pub struct HostConfig {
    #[serde(rename = "Binds", skip_serializing_if = "Vec::is_empty")]
    pub binds: Vec<String>,
    #[serde(rename = "PortBindings", skip_serializing_if = "BTreeMap::is_empty")]
    pub port_bindings: BTreeMap<String, Vec<HostPortBinding>>,
}

#[derive(Debug, Serialize)]
pub struct HostPortBinding {
    #[serde(rename = "HostPort")]
    pub host_port: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateContainerResponse {
    #[serde(rename = "Id")]
    pub id: String,
    #[serde(rename = "Warnings", default)]
    pub _warnings: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct InspectContainerResponse {
    #[serde(rename = "Id")]
    pub id: String,
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Config")]
    pub config: InspectContainerConfig,
    #[serde(rename = "State")]
    pub state: InspectContainerState,
}

#[derive(Debug, Deserialize)]
pub struct InspectContainerConfig {
    #[serde(rename = "Image")]
    pub image: String,
    #[serde(rename = "Labels", default)]
    pub labels: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct InspectContainerState {
    #[serde(rename = "Status")]
    pub status: String,
    #[serde(rename = "Running")]
    pub running: bool,
}
