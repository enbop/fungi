use fungi_config::{FungiConfig, FungiDir};
use fungi_daemon_grpc::fungi_daemon_grpc::fungi_daemon_client::FungiDaemonClient;

use crate::commands::CommonArgs;

pub async fn get_rpc_client(
    args: &CommonArgs,
) -> Option<FungiDaemonClient<tonic::transport::Channel>> {
    let fungi_config = FungiConfig::try_read_from_dir(&args.fungi_dir()).ok()?;
    let rpc_addr = format!("http://{}", fungi_config.rpc.listen_address);

    let connect_timeout = std::time::Duration::from_secs(3);
    match tokio::time::timeout(connect_timeout, FungiDaemonClient::connect(rpc_addr)).await {
        Ok(Ok(client)) => Some(client),
        Ok(Err(e)) => {
            eprintln!("Cannot connect to Fungi daemon. Is it running?");
            log::error!("Error connecting to daemon: {}", e);
            None
        }
        Err(_) => {
            eprintln!("Cannot connect to Fungi daemon. Is it running?");
            log::error!(
                "Connection timeout after {} seconds",
                connect_timeout.as_secs()
            );
            None
        }
    }
}
