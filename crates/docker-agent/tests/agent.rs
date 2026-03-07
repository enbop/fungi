use fungi_docker_agent::{
    AgentPolicy, BindMount, ContainerSpec, DockerAgent, LogsOptions, PortBinding, PortRule,
};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};
use tempfile::{TempDir, tempdir};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixListener,
    sync::Mutex,
};

#[derive(Clone, Debug)]
struct RecordedRequest {
    method: String,
    path: String,
    body: Vec<u8>,
}

#[tokio::test]
async fn creates_and_starts_managed_container() {
    let fixture = ServerFixture::start().await;
    let agent = DockerAgent::new(sample_policy(fixture.socket_path.clone()));

    let spec = ContainerSpec {
        name: Some("filebrowser".into()),
        image: "filebrowser/filebrowser:latest".into(),
        env: BTreeMap::from([(String::from("FB_NOAUTH"), String::from("true"))]),
        mounts: vec![BindMount {
            host_path: PathBuf::from("/tmp/fungi/data"),
            container_path: "/srv".into(),
        }],
        ports: vec![PortBinding {
            host_port: 8080,
            container_port: 80,
            protocol: Default::default(),
        }],
        ..Default::default()
    };

    let details = agent.create_container(&spec).await.unwrap();
    assert_eq!(details.name, "filebrowser");
    agent.start_container("container-1").await.unwrap();

    let requests = fixture.requests.lock().await.clone();
    assert_eq!(requests[0].method, "POST");
    assert!(requests[0].path.starts_with("/containers/create?name=filebrowser"));
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["Image"], "filebrowser/filebrowser:latest");
    assert_eq!(body["Labels"]["managed_by"], "fungi");
    assert_eq!(requests.last().unwrap().path, "/containers/container-1/start");
}

#[tokio::test]
async fn rejects_unmanaged_container_operations() {
    let fixture = ServerFixture::start_unmanaged().await;
    let agent = DockerAgent::new(sample_policy(fixture.socket_path.clone()));

    let err = agent.start_container("legacy").await.unwrap_err();
    assert!(err.to_string().contains("not managed by fungi"));
}

#[tokio::test]
async fn fetches_logs() {
    let fixture = ServerFixture::start().await;
    let agent = DockerAgent::new(sample_policy(fixture.socket_path.clone()));

    let logs = agent.container_logs("container-1", &LogsOptions::default()).await.unwrap();
    assert!(logs.text.contains("hello"));
}

fn sample_policy(socket_path: PathBuf) -> AgentPolicy {
    AgentPolicy {
        socket_path,
        managed_label_key: "managed_by".into(),
        managed_label_value: "fungi".into(),
        allowed_host_paths: vec![PathBuf::from("/tmp/fungi")],
        allowed_ports: vec![PortRule::Single(8080)],
    }
}

struct ServerFixture {
    _dir: TempDir,
    socket_path: PathBuf,
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
}

impl ServerFixture {
    async fn start() -> Self {
        Self::spawn(false).await
    }

    async fn start_unmanaged() -> Self {
        Self::spawn(true).await
    }

    async fn spawn(unmanaged: bool) -> Self {
        let dir = tempdir().unwrap();
        let socket_path = dir.path().join("docker.sock");
        let listener = UnixListener::bind(&socket_path).unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = requests.clone();

        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let recorded = recorded.clone();
                tokio::spawn(async move {
                    let request = read_request(&mut stream).await.unwrap();
                    recorded.lock().await.push(request.clone());
                    let response = response_for(&request, unmanaged);
                    let _ = stream.write_all(response.as_bytes()).await;
                });
            }
        });
        Self {
            _dir: dir,
            socket_path,
            requests,
        }
    }
}

async fn read_request(stream: &mut tokio::net::UnixStream) -> std::io::Result<RecordedRequest> {
    let mut buffer = Vec::new();
    let mut header_end = None;
    while header_end.is_none() {
        let mut chunk = [0_u8; 1024];
        let size = stream.read(&mut chunk).await?;
        if size == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..size]);
        header_end = find_header_end(&buffer);
    }

    let header_end = header_end.unwrap();
    let headers = &buffer[..header_end];
    let header_text = String::from_utf8_lossy(headers);
    let mut lines = header_text.split("\r\n");
    let request_line = lines.next().unwrap();
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts.next().unwrap().to_string();
    let path = request_parts.next().unwrap().to_string();

    let content_length = lines
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            if name.eq_ignore_ascii_case("content-length") {
                return value.trim().parse::<usize>().ok();
            }
            None
        })
        .unwrap_or(0);

    let mut body = buffer[header_end + 4..].to_vec();
    while body.len() < content_length {
        let mut chunk = vec![0_u8; content_length - body.len()];
        let size = stream.read(&mut chunk).await?;
        if size == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..size]);
    }
    body.truncate(content_length);

    Ok(RecordedRequest { method, path, body })
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|window| window == b"\r\n\r\n")
}

fn response_for(request: &RecordedRequest, unmanaged: bool) -> String {
    match (request.method.as_str(), request.path.as_str()) {
        ("POST", path) if path.starts_with("/containers/create") => http_response(
            201,
            r#"{"Id":"container-1","Warnings":[]}"#,
        ),
        ("GET", "/containers/container-1/json") => http_response(
            200,
            r#"{"Id":"container-1","Name":"/filebrowser","Config":{"Image":"filebrowser/filebrowser:latest","Labels":{"managed_by":"fungi"}},"State":{"Status":"created","Running":false}}"#,
        ),
        ("POST", "/containers/container-1/start") => http_response(204, ""),
        ("GET", path) if path.starts_with("/containers/container-1/logs") => http_response_bytes(
            200,
            &[1, 0, 0, 0, 0, 0, 0, 6, b'h', b'e', b'l', b'l', b'o', b'\n'],
        ),
        ("GET", "/containers/legacy/json") if unmanaged => http_response(
            200,
            r#"{"Id":"legacy","Name":"/legacy","Config":{"Image":"busybox","Labels":{"owner":"user"}},"State":{"Status":"running","Running":true}}"#,
        ),
        _ => http_response(404, r#"{"message":"not found"}"#),
    }
}

fn http_response(status: u16, body: &str) -> String {
    format!(
        "HTTP/1.1 {} OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{}",
        status,
        body.len(),
        body
    )
}

fn http_response_bytes(status: u16, body: &[u8]) -> String {
    let mut response = format!(
        "HTTP/1.1 {} OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        status,
        body.len()
    )
    .into_bytes();
    response.extend_from_slice(body);
    String::from_utf8_lossy(&response).to_string()
}