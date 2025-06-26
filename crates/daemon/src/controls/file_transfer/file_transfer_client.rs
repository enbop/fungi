use std::{
    collections::HashMap, convert::Infallible, net::IpAddr, path::PathBuf, sync::Arc,
    time::Duration,
};

use anyhow::bail;
use dav_server::DavHandler;
use fungi_config::file_transfer::FileTransferClient as FileTransferClientConfig;
use fungi_swarm::SwarmControl;
use fungi_util::protocols::FUNGI_FILE_TRANSFER_PROTOCOL;
use hyper::{server::conn::http1, service::service_fn};
use hyper_util::rt::TokioIo;
use libp2p::{PeerId, Stream};
use parking_lot::Mutex;
use tarpc::{context, serde_transport, tokio_serde::formats::Bincode};
use tokio::net::TcpListener;
use tokio_util::{codec::LengthDelimitedCodec, compat::FuturesAsyncReadCompatExt as _};

use crate::controls::file_transfer::{FileTransferRpc, FileTransferRpcClient};

#[derive(Debug, Clone)]
struct FileClient {
    peer_id: Arc<PeerId>,
    is_windows: Option<bool>,
    rpc_client: Option<FileTransferRpcClient>,
}

#[derive(Clone)]
pub struct FileTransferClientControl {
    swarm_control: SwarmControl,
    clients: Arc<Mutex<HashMap<String, FileClient>>>,
}

impl std::fmt::Debug for FileTransferClientControl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // only print name "FileTransferClientControl"
        f.debug_struct("FileTransferClientControl").finish()
    }
}

impl FileTransferClientControl {
    pub fn new(swarm_control: SwarmControl) -> Self {
        Self {
            swarm_control,
            clients: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn connect_and_get_host_name(
        &self,
        peer_id: PeerId,
    ) -> anyhow::Result<Option<String>> {
        self.swarm_control.connect(peer_id).await?;
        let host_name = self
            .swarm_control
            .state()
            .connected_peers()
            .lock()
            .get(&peer_id)
            .expect("Peer should be connected.")
            .host_name();
        Ok(host_name)
    }

    // TODO add callback on_client_broken

    pub fn has_client(&self, peer_id: &PeerId) -> bool {
        self.clients
            .lock()
            .values()
            .any(|fc| fc.peer_id.as_ref() == peer_id)
    }

    pub fn add_client(&self, config: FileTransferClientConfig) {
        let key = if config.name.is_some() {
            config.name.clone().unwrap()
        } else {
            config.peer_id.to_string()
        };
        log::info!(
            "Adding file transfer client: {} with peer_id: {}",
            key,
            config.peer_id
        );
        self.clients.lock().insert(
            key,
            FileClient {
                peer_id: Arc::new(config.peer_id),
                rpc_client: None,
                is_windows: None,
            },
        );
    }

    pub fn remove_client(&self, peer_id: &PeerId) {
        log::info!("Removing file transfer client with peer_id: {}", peer_id);
        self.clients
            .lock()
            .retain(|_, fc| !fc.peer_id.as_ref().eq(peer_id));
    }

    async fn connect_client(&self, peer_id: PeerId) -> anyhow::Result<FileTransferRpcClient> {
        self.swarm_control.connect(peer_id).await?;
        let mut stream_control = self.swarm_control.stream_control().clone();
        let Ok(stream) = stream_control
            .open_stream(peer_id.clone(), FUNGI_FILE_TRANSFER_PROTOCOL)
            .await
        else {
            bail!("Failed to open stream to peer {}", peer_id);
        };
        let client = connect_file_transfer_rpc(stream);

        Ok(client)
    }

    async fn get_client(&self, name: &str) -> anyhow::Result<FileTransferRpcClient> {
        let Some(mut fc) = self.clients.lock().get(name).cloned() else {
            bail!("File transfer client with name '{}' not found", name);
        };
        if fc.rpc_client.is_none() {
            let client = self.connect_client(*fc.peer_id).await?;
            let is_windows = client.is_windows(context::current()).await?;
            fc.is_windows = Some(is_windows);
            fc.rpc_client = Some(client);
            // update the client in the map
            self.clients.lock().insert(name.to_string(), fc.clone());
        }
        if fc.rpc_client.is_none() {
            bail!("File transfer client with name '{}' is not connected", name);
        }
        Ok(fc.rpc_client.unwrap())
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
impl FileTransferClientControl {
    async fn extract_client_and_path(
        &self,
        path: PathBuf,
    ) -> anyhow::Result<(FileTransferRpcClient, PathBuf)> {
        let path_str = path.to_string_lossy().to_string();
        let clean_path = path_str
            .trim_start_matches("./")
            .trim_start_matches("/")
            .to_string();

        if clean_path.is_empty() || clean_path == "." {
            bail!("Cannot perform operation on root directory. Please specify a client directory.");
        }

        let parts: Vec<&str> = clean_path.split('/').collect();
        let client_name = parts[0];

        let client = self.get_client(client_name).await?;

        let mut remaining_path = PathBuf::new();
        if parts.len() > 1 {
            for part in &parts[1..] {
                remaining_path.push(part);
            }
        } else {
            remaining_path.push(".");
        }

        Ok((client, remaining_path))
    }

    // TODO better way to check root path
    pub async fn metadata(&self, path: PathBuf) -> fungi_fs::Result<fungi_fs::Metadata> {
        let path_str = path.to_string_lossy();
        if path_str == "." || path_str == "./" || path_str == "/" {
            return Ok(fungi_fs::Metadata {
                is_dir: true,
                is_file: false,
                len: 0,
                modified: Some(std::time::SystemTime::now()),
                is_symlink: false,
                gid: 0,
                uid: 0,
                links: 1,
                permissions: 0o555,
                readlink: None,
            });
        }

        let (client, remaining_path) = match self.extract_client_and_path(path).await {
            Ok(result) => result,
            Err(e) => return Err(fungi_fs::FileTransferError::Other(e.to_string())),
        };

        client
            .metadata(context::current(), remaining_path)
            .await
            .map_err(|_| fungi_fs::FileTransferError::ConnectionBroken)?
    }

    pub async fn list(&self, path: PathBuf) -> fungi_fs::Result<Vec<fungi_fs::FileInfo>> {
        let path_str = path.to_string_lossy();
        if path_str == "." || path_str == "./" || path_str == "/" {
            let clients = self.clients.lock();
            let mut result = Vec::new();

            for client_name in clients.keys() {
                result.push(fungi_fs::FileInfo {
                    path: PathBuf::from(client_name),
                    metadata: fungi_fs::Metadata {
                        is_dir: true,
                        is_file: false,
                        len: 0,
                        modified: Some(std::time::SystemTime::now()),
                        is_symlink: false,
                        gid: 0,
                        uid: 0,
                        links: 1,
                        permissions: 0o555,
                        readlink: None,
                    },
                });
            }

            return Ok(result);
        }

        let (client, remaining_path) = match self.extract_client_and_path(path).await {
            Ok(result) => result,
            Err(e) => return Err(fungi_fs::FileTransferError::Other(e.to_string())),
        };

        client
            .list(context::current(), remaining_path)
            .await
            .map_err(|_| fungi_fs::FileTransferError::ConnectionBroken)?
    }

    pub async fn get(&self, path: PathBuf, start_pos: u64) -> fungi_fs::Result<Vec<u8>> {
        let path_str = path.to_string_lossy();
        if path_str == "." || path_str == "./" || path_str == "/" {
            return Err(fungi_fs::FileTransferError::Other(
                "Cannot read from root directory".to_string(),
            ));
        }

        let (client, remaining_path) = match self.extract_client_and_path(path).await {
            Ok(result) => result,
            Err(e) => return Err(fungi_fs::FileTransferError::Other(e.to_string())),
        };

        client
            .get(context::current(), remaining_path, start_pos)
            .await
            .map_err(|_| fungi_fs::FileTransferError::ConnectionBroken)?
    }

    pub async fn put(
        &self,
        bytes: Vec<u8>,
        path: PathBuf,
        start_pos: u64,
    ) -> fungi_fs::Result<u64> {
        let path_str = path.to_string_lossy();
        if path_str == "." || path_str == "./" || path_str == "/" {
            return Err(fungi_fs::FileTransferError::Other(
                "Cannot write to root directory".to_string(),
            ));
        }

        let (client, remaining_path) = match self.extract_client_and_path(path).await {
            Ok(result) => result,
            Err(e) => return Err(fungi_fs::FileTransferError::Other(e.to_string())),
        };

        client
            .put(context::current(), bytes, remaining_path, start_pos)
            .await
            .map_err(|_| fungi_fs::FileTransferError::ConnectionBroken)?
    }

    pub async fn del(&self, path: PathBuf) -> fungi_fs::Result<()> {
        let path_str = path.to_string_lossy();
        if path_str == "." || path_str == "./" || path_str == "/" {
            return Err(fungi_fs::FileTransferError::Other(
                "Cannot delete root directory".to_string(),
            ));
        }

        let (client, remaining_path) = match self.extract_client_and_path(path).await {
            Ok(result) => result,
            Err(e) => return Err(fungi_fs::FileTransferError::Other(e.to_string())),
        };

        if remaining_path.to_string_lossy() == "." {
            return Err(fungi_fs::FileTransferError::Other(
                "Cannot delete client root directory".to_string(),
            ));
        }

        client
            .del(context::current(), remaining_path)
            .await
            .map_err(|_| fungi_fs::FileTransferError::ConnectionBroken)?
    }

    pub async fn rmd(&self, path: PathBuf) -> fungi_fs::Result<()> {
        let path_str = path.to_string_lossy();
        if path_str == "." || path_str == "./" || path_str == "/" {
            return Err(fungi_fs::FileTransferError::Other(
                "Cannot remove root directory".to_string(),
            ));
        }

        let (client, remaining_path) = match self.extract_client_and_path(path).await {
            Ok(result) => result,
            Err(e) => return Err(fungi_fs::FileTransferError::Other(e.to_string())),
        };

        if remaining_path.to_string_lossy() == "." {
            return Err(fungi_fs::FileTransferError::Other(
                "Cannot remove client root directory".to_string(),
            ));
        }

        client
            .rmd(context::current(), remaining_path)
            .await
            .map_err(|_| fungi_fs::FileTransferError::ConnectionBroken)?
    }

    pub async fn mkd(&self, path: PathBuf) -> fungi_fs::Result<()> {
        let path_str = path.to_string_lossy();
        if path_str == "." || path_str == "./" || path_str == "/" {
            return Err(fungi_fs::FileTransferError::Other(
                "Cannot create directory in root".to_string(),
            ));
        }

        let (client, remaining_path) = match self.extract_client_and_path(path).await {
            Ok(result) => result,
            Err(e) => return Err(fungi_fs::FileTransferError::Other(e.to_string())),
        };

        client
            .mkd(context::current(), remaining_path)
            .await
            .map_err(|_| fungi_fs::FileTransferError::ConnectionBroken)?
    }
    pub async fn rename(&self, from: PathBuf, to: PathBuf) -> fungi_fs::Result<()> {
        let from_str = from.to_string_lossy();
        let to_str = to.to_string_lossy();

        if from_str == "."
            || from_str == "./"
            || from_str == "/"
            || to_str == "."
            || to_str == "./"
            || to_str == "/"
        {
            return Err(fungi_fs::FileTransferError::Other(
                "Cannot rename root directory".to_string(),
            ));
        }

        let from_clean = from_str.trim_start_matches("./").to_string();
        let to_clean = to_str.trim_start_matches("./").to_string();

        if !from_clean.contains('/') || !to_clean.contains('/') {
            return Err(fungi_fs::FileTransferError::Other(
                "Cannot rename client directories at the top level".to_string(),
            ));
        }

        let (from_client, from_remaining_path) =
            match self.extract_client_and_path(from.clone()).await {
                Ok(result) => result,
                Err(e) => return Err(fungi_fs::FileTransferError::Other(e.to_string())),
            };

        let (to_client, to_remaining_path) = match self.extract_client_and_path(to.clone()).await {
            Ok(result) => result,
            Err(e) => return Err(fungi_fs::FileTransferError::Other(e.to_string())),
        };

        if from.components().next() != to.components().next() {
            return Err(fungi_fs::FileTransferError::Other(
                "Cannot rename across different clients".to_string(),
            ));
        }

        if from_remaining_path.to_string_lossy() == "." {
            return Err(fungi_fs::FileTransferError::Other(
                "Cannot rename client root directory".to_string(),
            ));
        }

        from_client
            .rename(context::current(), from_remaining_path, to_remaining_path)
            .await
            .map_err(|_| fungi_fs::FileTransferError::ConnectionBroken)?
    }

    pub async fn cwd(&self, path: PathBuf) -> fungi_fs::Result<()> {
        let path_str = path.to_string_lossy();
        if path_str == "." || path_str == "./" || path_str == "/" {
            return Ok(());
        }

        let (client, remaining_path) = match self.extract_client_and_path(path).await {
            Ok(result) => result,
            Err(e) => return Err(fungi_fs::FileTransferError::Other(e.to_string())),
        };

        client
            .cwd(context::current(), remaining_path)
            .await
            .map_err(|_| fungi_fs::FileTransferError::ConnectionBroken)?
    }
}

pub async fn start_ftp_proxy_service(host: IpAddr, port: u16, client: FileTransferClientControl) {
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

pub async fn start_webdav_proxy_service(
    host: IpAddr,
    port: u16,
    client: FileTransferClientControl,
) {
    let dav_server = DavHandler::builder()
        .filesystem(Box::new(client))
        .build_handler();

    let addr = format!("{}:{}", host, port);
    println!("Listening webdav on {addr}");
    let listener = TcpListener::bind(addr).await.unwrap();

    // We start a loop to continuously accept incoming connections
    loop {
        let (stream, _) = listener.accept().await.unwrap();
        let dav_server = dav_server.clone();

        // Use an adapter to access something implementing `tokio::io` traits as if they implement
        // `hyper::rt` IO traits.
        let io = TokioIo::new(stream);

        // Spawn a tokio task to serve multiple connections concurrently
        tokio::task::spawn(async move {
            // Finally, we bind the incoming connection to our `hello` service
            if let Err(err) = http1::Builder::new()
                // `service_fn` converts our function in a `Service`
                .serve_connection(
                    io,
                    service_fn({
                        move |req| {
                            let dav_server = dav_server.clone();
                            async move { Ok::<_, Infallible>(dav_server.handle(req).await) }
                        }
                    }),
                )
                .await
            {
                eprintln!("Failed serving: {err:?}");
            }
        });
    }
}
