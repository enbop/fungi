use fungi_config::{FungiConfig, FungiDir};
use fungi_daemon_grpc::Request;
use fungi_daemon_grpc::fungi_daemon_grpc::Empty;
use fungi_daemon_grpc::fungi_daemon_grpc::fungi_daemon_client::FungiDaemonClient;

use crate::commands::CommonArgs;

use super::shared::fatal;

pub async fn get_rpc_client(
    args: &CommonArgs,
) -> Option<FungiDaemonClient<tonic::transport::Channel>> {
    let fungi_config = match FungiConfig::try_read_from_dir(&args.fungi_dir()) {
        Ok(config) => config,
        Err(error) => fatal(format!("Failed to read configuration: {error}")),
    };
    let expected_config_path = fungi_config.config_file_path().to_path_buf();
    let rpc_addr = format!("http://{}", fungi_config.rpc.listen_address);

    let connect_timeout = std::time::Duration::from_secs(3);
    match tokio::time::timeout(connect_timeout, FungiDaemonClient::connect(rpc_addr)).await {
        Ok(Ok(mut client)) => match client.config_file_path(Request::new(Empty {})).await {
            Ok(resp) => {
                let remote_config_path =
                    std::path::PathBuf::from(resp.into_inner().config_file_path);
                if remote_config_path == expected_config_path {
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
