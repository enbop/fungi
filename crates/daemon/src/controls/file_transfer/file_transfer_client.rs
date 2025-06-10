use std::fmt::Debug;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use fungi_config::{FungiConfig, file_transfer::FileTransferClient as FileTransferClientConfig};
use fungi_swarm::SwarmControl;
use fungi_util::protocols::FUNGI_FILE_TRANSFER_PROTOCOL;
use libp2p::{PeerId, Stream};
use libp2p_stream::Control;
use libunftp::{auth::UserDetail, storage::StorageBackend};
use tarpc::{context, serde_transport, tokio_serde::formats::Bincode};
use tokio::task::JoinHandle;
use tokio_util::{codec::LengthDelimitedCodec, compat::FuturesAsyncReadCompatExt as _};

use super::FileTransferRpcClient;

#[async_trait]
impl<User: UserDetail> StorageBackend<User> for FileTransferRpcClient {
    type Metadata = fungi_fs::Metadata;

    fn supported_features(&self) -> u32 {
        libunftp::storage::FEATURE_RESTART
    }

    async fn metadata<P: AsRef<std::path::Path> + Send + Debug>(
        &self,
        _user: &User,
        path: P,
    ) -> libunftp::storage::Result<Self::Metadata> {
        let path = path.as_ref().to_path_buf();
        // TODO handle errors properly
        self.metadata(context::current(), path)
            .await
            .unwrap()
            .map_err(|e| map_error(e))
    }

    async fn list<P: AsRef<std::path::Path> + Send + Debug>(
        &self,
        _user: &User,
        path: P,
    ) -> libunftp::storage::Result<
        Vec<libunftp::storage::Fileinfo<std::path::PathBuf, Self::Metadata>>,
    > {
        let path = path.as_ref().to_path_buf();
        let file_infos = self
            .list(context::current(), path)
            .await
            .unwrap()
            .map_err(|e| map_error(e))?;

        Ok(file_infos
            .into_iter()
            .map(|info| libunftp::storage::Fileinfo {
                path: info.path,
                metadata: info.metadata,
            })
            .collect())
    }

    async fn get<P: AsRef<std::path::Path> + Send + Debug>(
        &self,
        _user: &User,
        path: P,
        start_pos: u64,
    ) -> libunftp::storage::Result<Box<dyn tokio::io::AsyncRead + Send + Sync + Unpin>> {
        let path = path.as_ref().to_path_buf();
        let bytes = self
            .get(context::current(), path, start_pos)
            .await
            .unwrap()
            .map_err(|e| map_error(e))?;

        let cursor = std::io::Cursor::new(bytes);
        Ok(Box::new(cursor) as Box<dyn tokio::io::AsyncRead + Send + Sync + Unpin>)
    }

    async fn put<
        P: AsRef<std::path::Path> + Send,
        R: tokio::io::AsyncRead + Send + Sync + 'static + Unpin,
    >(
        &self,
        _user: &User,
        mut bytes: R,
        path: P,
        start_pos: u64,
    ) -> libunftp::storage::Result<u64> {
        let path = path.as_ref().to_path_buf();

        let mut buffer = Vec::new();
        tokio::io::copy(&mut bytes, &mut buffer)
            .await
            .map_err(|e| {
                libunftp::storage::Error::new(libunftp::storage::ErrorKind::LocalError, e)
            })?;

        self.put(context::current(), buffer, path, start_pos)
            .await
            .unwrap()
            .map_err(|e| map_error(e))
    }

    async fn del<P: AsRef<std::path::Path> + Send + Debug>(
        &self,
        _user: &User,
        path: P,
    ) -> libunftp::storage::Result<()> {
        let path = path.as_ref().to_path_buf();
        self.del(context::current(), path)
            .await
            .unwrap()
            .map_err(|e| map_error(e))
    }

    async fn rmd<P: AsRef<std::path::Path> + Send + Debug>(
        &self,
        _user: &User,
        path: P,
    ) -> libunftp::storage::Result<()> {
        let path = path.as_ref().to_path_buf();
        self.rmd(context::current(), path)
            .await
            .unwrap()
            .map_err(|e| map_error(e))
    }

    async fn mkd<P: AsRef<std::path::Path> + Send + Debug>(
        &self,
        _user: &User,
        path: P,
    ) -> libunftp::storage::Result<()> {
        let path = path.as_ref().to_path_buf();
        self.mkd(context::current(), path)
            .await
            .unwrap()
            .map_err(|e| map_error(e))
    }

    async fn rename<P: AsRef<std::path::Path> + Send + Debug>(
        &self,
        _user: &User,
        from: P,
        to: P,
    ) -> libunftp::storage::Result<()> {
        let from = from.as_ref().to_path_buf();
        let to = to.as_ref().to_path_buf();
        self.rename(context::current(), from, to)
            .await
            .unwrap()
            .map_err(|e| map_error(e))
    }

    async fn cwd<P: AsRef<std::path::Path> + Send + Debug>(
        &self,
        _user: &User,
        path: P,
    ) -> libunftp::storage::Result<()> {
        let path = path.as_ref().to_path_buf();
        self.cwd(context::current(), path)
            .await
            .unwrap()
            .map_err(|e| map_error(e))
    }
}

fn map_error(err: fungi_fs::FileTransferError) -> libunftp::storage::Error {
    use fungi_fs::FileTransferError;
    use libunftp::storage::ErrorKind;

    match err {
        FileTransferError::NotFound => ErrorKind::PermanentFileNotAvailable.into(),
        FileTransferError::PermissionDenied => ErrorKind::PermissionDenied.into(),
        FileTransferError::Other(msg) => {
            log::error!("File transfer error: {}", msg);
            ErrorKind::LocalError.into()
        }
    }
}
#[derive(Clone)]
pub struct FileTransferClientControl {
    swarm_control: SwarmControl,
    clients: Arc<Mutex<HashMap<PeerId, JoinHandle<()>>>>,
}

impl FileTransferClientControl {
    pub fn new(swarm_control: SwarmControl) -> Self {
        Self {
            swarm_control,
            clients: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn start_client(&self, config: FileTransferClientConfig) {
        let client_handle = tokio::spawn(start_file_transfer_client(
            config.clone(),
            self.swarm_control.clone(),
        ));
        self.clients
            .lock()
            .unwrap()
            .insert(config.target_peer, client_handle);
    }
}

fn connect_file_transfer_rpc(stream: Stream) -> FileTransferRpcClient {
    let codec_builder = LengthDelimitedCodec::builder();
    let transport = serde_transport::new(
        codec_builder.new_framed(stream.compat()),
        Bincode::default(),
    );
    FileTransferRpcClient::new(Default::default(), transport).spawn()
}

async fn start_file_transfer_client(
    config: FileTransferClientConfig,
    mut swarm_control: SwarmControl,
) {
    loop {
        if let Err(e) = swarm_control
            .invoke_swarm(move |swarm| swarm.dial(config.target_peer))
            .await
            .unwrap()
        {
            log::error!(
                "Failed to dial peer {}: {}. Retrying in 5 seconds...",
                config.target_peer,
                e
            );
            tokio::time::sleep(Duration::from_secs(5)).await;
            continue;
        };

        let Ok(stream) = swarm_control
            .stream_control
            .open_stream(config.target_peer, FUNGI_FILE_TRANSFER_PROTOCOL)
            .await
        else {
            log::error!("Failed to open stream to peer {}", config.target_peer);
            tokio::time::sleep(Duration::from_secs(5)).await;
            continue;
        };
        let client = connect_file_transfer_rpc(stream);
        start_ftp_proxy_service(config.proxy_ftp_host.clone(), config.proxy_ftp_port, client).await;
    }
}

async fn start_ftp_proxy_service(host: String, port: u16, client: FileTransferRpcClient) {
    loop {
        let client_cl = client.clone();
        let server = libunftp::ServerBuilder::new(Box::new(move || client_cl.clone()))
            .greeting("Welcome to Fungi FTP proxy")
            .passive_ports(50000..=65535)
            .build()
            .unwrap();

        log::info!("Starting FTP proxy service on port {}", port);
        if let Err(e) = server.listen(format!("{}:{}", host, port)).await {
            log::error!(
                "Failed to start FTP proxy service on port {}: {}. Retrying in 5 seconds...",
                port,
                e
            );
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}
