pub mod fungi_daemon_grpc {
    tonic::include_proto!("fungi_daemon");
}

use std::net::SocketAddr;

use fungi_daemon_grpc::fungi_daemon_server::FungiDaemon;
use fungi_daemon_grpc::{Empty, HostnameResponse};
pub use tonic::{Request, Response, Status};

pub async fn start_grpc_server(
    daemon: fungi_daemon::FungiDaemon,
    addr: SocketAddr,
) -> anyhow::Result<()> {
    tonic::transport::Server::builder()
        .add_service(
            fungi_daemon_grpc::fungi_daemon_server::FungiDaemonServer::new(
                FungiDaemonRpcImpl::new(daemon),
            ),
        )
        .serve(addr)
        .await?;
    Ok(())
}

pub struct FungiDaemonRpcImpl {
    inner: fungi_daemon::FungiDaemon,
}

impl FungiDaemonRpcImpl {
    pub fn new(inner: fungi_daemon::FungiDaemon) -> Self {
        Self { inner }
    }
}

#[tonic::async_trait]
impl FungiDaemon for FungiDaemonRpcImpl {
    async fn hostname(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<HostnameResponse>, Status> {
        let response = HostnameResponse {
            hostname: self.inner.host_name(),
        };
        Ok(Response::new(response))
    }
}
