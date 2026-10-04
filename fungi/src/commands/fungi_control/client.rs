use fungi_config::{FungiConfig, FungiDir};
use fungi_daemon_grpc::Request;
use fungi_daemon_grpc::fungi_daemon_grpc::Empty;
use fungi_daemon_grpc::fungi_daemon_grpc::fungi_daemon_client::FungiDaemonClient;

use crate::commands::CommonArgs;

use super::shared::fatal;

pub(super) const DEFAULT_RPC_REQUEST_TIMEOUT: std::time::Duration =
    std::time::Duration::from_secs(30);
pub(super) const LONG_RPC_REQUEST_TIMEOUT: std::time::Duration =
    std::time::Duration::from_secs(300);

pub(super) fn read_rpc_endpoint(fungi_dir: &std::path::Path) -> anyhow::Result<String> {
    fungi_config::read_daemon_endpoint(fungi_dir)
}

pub(super) fn rpc_address_from_endpoint(endpoint: &str) -> anyhow::Result<String> {
    let address = endpoint
        .strip_prefix("http://")
        .ok_or_else(|| anyhow::anyhow!("Unsupported daemon endpoint transport: {endpoint}"))?;
    let address: std::net::SocketAddr = address
        .parse()
        .map_err(|error| anyhow::anyhow!("Invalid daemon endpoint {endpoint}: {error}"))?;
    Ok(address.to_string())
}

fn rpc_endpoint(
    rpc_addr: String,
    connect_timeout: std::time::Duration,
    request_timeout: std::time::Duration,
) -> anyhow::Result<tonic::transport::Endpoint> {
    Ok(tonic::transport::Endpoint::from_shared(rpc_addr)?
        .connect_timeout(connect_timeout)
        .timeout(request_timeout))
}

pub async fn get_rpc_client(
    args: &CommonArgs,
) -> Option<FungiDaemonClient<tonic::transport::Channel>> {
    get_rpc_client_with_timeout(args, DEFAULT_RPC_REQUEST_TIMEOUT).await
}

pub(super) async fn get_rpc_client_with_timeout(
    args: &CommonArgs,
    request_timeout: std::time::Duration,
) -> Option<FungiDaemonClient<tonic::transport::Channel>> {
    let fungi_config = match FungiConfig::try_read_from_dir(&args.fungi_dir()) {
        Ok(config) => config,
        Err(error) => fatal(format!("Failed to read configuration: {error}")),
    };
    let expected_config_path = fungi_config.config_file_path().to_path_buf();
    let rpc_addr = match read_rpc_endpoint(&args.fungi_dir()) {
        Ok(endpoint) => endpoint,
        Err(error) => fatal(format!("Failed to discover Fungi daemon: {error}")),
    };

    let connect_timeout = std::time::Duration::from_secs(3);
    let endpoint = rpc_endpoint(rpc_addr, connect_timeout, request_timeout)
        .unwrap_or_else(|error| fatal(format!("Invalid Fungi daemon endpoint: {error}")));
    match tokio::time::timeout(connect_timeout, endpoint.connect()).await {
        Ok(Ok(channel)) => {
            let mut client = FungiDaemonClient::new(channel);
            match client.config_file_path(Request::new(Empty {})).await {
                Ok(resp) => {
                    let remote_config_path =
                        std::path::PathBuf::from(resp.into_inner().config_file_path);
                    if config_paths_match(&remote_config_path, &expected_config_path) {
                        Some(client)
                    } else {
                        log::warn!(
                            "Connected daemon config path mismatch: expected {}, got {}",
                            expected_config_path.display(),
                            remote_config_path.display()
                        );
                        None
                    }
                }
                Err(error) => {
                    log::error!("Failed to query daemon config path: {}", error);
                    None
                }
            }
        }
        Ok(Err(e)) => {
            log::error!("Error connecting to daemon: {}", e);
            None
        }
        Err(_) => {
            log::error!(
                "Connection timeout after {} seconds",
                connect_timeout.as_secs()
            );
            None
        }
    }
}

fn config_paths_match(left: &std::path::Path, right: &std::path::Path) -> bool {
    if left == right {
        return true;
    }

    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use std::{convert::Infallible, future::Future, pin::Pin, time::Duration};

    use fungi_daemon_grpc::fungi_daemon_grpc::fungi_daemon_client::FungiDaemonClient;
    use tonic::{Request, body::Body, codegen::Service, server::NamedService};

    use super::{config_paths_match, read_rpc_endpoint, rpc_address_from_endpoint, rpc_endpoint};

    #[derive(Clone)]
    struct HangingRpcService;

    impl NamedService for HangingRpcService {
        const NAME: &'static str = "fungi_daemon.FungiDaemon";
    }

    impl Service<tonic::codegen::http::Request<Body>> for HangingRpcService {
        type Response = tonic::codegen::http::Response<Body>;
        type Error = Infallible;
        type Future =
            Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

        fn poll_ready(
            &mut self,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), Self::Error>> {
            std::task::Poll::Ready(Ok(()))
        }

        fn call(&mut self, _request: tonic::codegen::http::Request<Body>) -> Self::Future {
            Box::pin(std::future::pending())
        }
    }

    #[tokio::test]
    async fn rpc_endpoint_times_out_a_stalled_request() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(
            tonic::transport::Server::builder()
                .add_service(HangingRpcService)
                .serve_with_incoming(
                    tonic::codegen::tokio_stream::wrappers::TcpListenerStream::new(listener),
                ),
        );

        let channel = rpc_endpoint(
            format!("http://{address}"),
            Duration::from_secs(5),
            Duration::from_millis(50),
        )
        .unwrap()
        .connect()
        .await
        .unwrap();
        let mut client = FungiDaemonClient::new(channel);
        let response = tokio::time::timeout(
            Duration::from_secs(10),
            client.version(Request::new(fungi_daemon_grpc::fungi_daemon_grpc::Empty {})),
        )
        .await;
        server.abort();
        let error = response
            .expect("stalled RPC request must time out")
            .unwrap_err();
        assert!(error.message().contains("Timeout expired"));
    }

    #[test]
    fn config_path_match_accepts_relative_and_absolute_paths() {
        let cwd = std::env::current_dir().unwrap();
        let dir = tempfile::tempdir_in(&cwd).unwrap();
        let relative = dir.path().strip_prefix(&cwd).unwrap().join("config.toml");
        let absolute = cwd.join(&relative);
        std::fs::write(&absolute, "").unwrap();

        assert!(config_paths_match(&relative, &absolute));
    }

    #[test]
    fn rpc_address_is_derived_from_published_http_endpoint() {
        assert_eq!(
            rpc_address_from_endpoint("http://127.0.0.1:61234").unwrap(),
            "127.0.0.1:61234"
        );
    }

    #[test]
    fn invalid_rpc_endpoint_is_rejected() {
        let error = rpc_address_from_endpoint("127.0.0.1:61234").unwrap_err();
        assert!(error.to_string().contains("Unsupported daemon endpoint"));
    }

    #[test]
    fn missing_rpc_endpoint_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let error = read_rpc_endpoint(dir.path()).unwrap_err();
        assert!(error.to_string().contains("Failed to read daemon endpoint"));
    }
}
