use fungi_config::{FungiConfig, FungiDir};
use fungi_daemon_grpc::Request;
use fungi_daemon_grpc::fungi_daemon_grpc::Empty;
use fungi_daemon_grpc::fungi_daemon_grpc::fungi_daemon_client::FungiDaemonClient;

use crate::commands::CommonArgs;

use super::shared::fatal;

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

pub async fn get_rpc_client(
    args: &CommonArgs,
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
    match tokio::time::timeout(connect_timeout, FungiDaemonClient::connect(rpc_addr)).await {
        Ok(Ok(mut client)) => match client.config_file_path(Request::new(Empty {})).await {
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
        },
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
    use super::{config_paths_match, read_rpc_endpoint, rpc_address_from_endpoint};

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
