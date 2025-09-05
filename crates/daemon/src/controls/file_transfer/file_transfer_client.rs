use std::{
    collections::HashMap,
    convert::Infallible,
    net::IpAddr,
    ops::{Deref, DerefMut},
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime},
};

use anyhow::bail;
use dav_server::{DavHandler, fakels::FakeLs};
use fungi_config::file_transfer::FileTransferClient as FileTransferClientConfig;
use fungi_swarm::SwarmControl;
use fungi_util::protocols::FUNGI_FILE_TRANSFER_PROTOCOL;
use hyper::{server::conn::http1, service::service_fn};
use hyper_util::rt::TokioIo;
use libp2p::{PeerId, Stream};
use parking_lot::Mutex;
use tarpc::{client::RpcError, context, serde_transport, tokio_serde::formats::Bincode};
use tokio::net::TcpListener;
use tokio_util::{codec::LengthDelimitedCodec, compat::FuturesAsyncReadCompatExt as _};
use typed_path::{
    Utf8Component, Utf8Components, Utf8Path, Utf8PathBuf, Utf8UnixComponents, Utf8UnixEncoding,
};

use crate::controls::file_transfer::FileTransferRpcClient;

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ConnectedClient {
    peer_id: Arc<PeerId>,
    is_windows: bool,
    rpc_client: FileTransferRpcClient,
}

impl Deref for ConnectedClient {
    type Target = FileTransferRpcClient;

    fn deref(&self) -> &Self::Target {
        &self.rpc_client
    }
}

impl DerefMut for ConnectedClient {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.rpc_client
    }
}

#[derive(Debug, Clone)]
enum FileClientState {
    Connected(ConnectedClient),
    Disconnected,           // TODO add retry count
    Connecting(SystemTime), // start time of connection attempt
}

// #[derive(Debug, Clone)]
// struct FileClient {
//     peer_id: Arc<PeerId>,
//     is_windows: bool,
//     rpc_client: FileTransferRpcClient,
// }

#[derive(Clone)]
pub struct FileTransferClientsControl {
    swarm_control: SwarmControl,
    clients: Arc<Mutex<HashMap<String, (PeerId, FileClientState)>>>,
    /// Write buffer size for WebDAV operations
    write_buffer_size: usize,
}

impl std::fmt::Debug for FileTransferClientsControl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // only print name "FileTransferClientControl"
        f.debug_struct("FileTransferClientControl").finish()
    }
}

impl FileTransferClientsControl {
    const DEFAULT_WRITE_BUFFER_SIZE: usize = 1 * 1024 * 1024; // 1 MB

    pub fn new(swarm_control: SwarmControl) -> Self {
        Self::new_with_buffer_size(swarm_control, Self::DEFAULT_WRITE_BUFFER_SIZE)
    }

    pub fn new_with_buffer_size(swarm_control: SwarmControl, write_buffer_size: usize) -> Self {
        Self {
            swarm_control,
            clients: Arc::new(Mutex::new(HashMap::new())),
            write_buffer_size,
        }
    }

    pub fn write_buffer_size(&self) -> usize {
        self.write_buffer_size
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

    pub fn has_client(&self, peer_id: &PeerId) -> bool {
        self.clients.lock().values().any(|(id, _)| id == peer_id)
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
            (
                config.peer_id,
                FileClientState::Disconnected, // start with disconnected state
            ),
        );
    }

    pub fn remove_client(&self, peer_id: &PeerId) {
        log::info!("Removing file transfer client with peer_id: {peer_id}");
        self.clients.lock().retain(|_, (id, _)| id != peer_id);
    }

    async fn connect_client(&self, peer_id: PeerId) -> anyhow::Result<FileTransferRpcClient> {
        self.swarm_control.connect(peer_id).await?;
        let mut stream_control = self.swarm_control.stream_control().clone();
        let stream = match stream_control
            .open_stream(peer_id, FUNGI_FILE_TRANSFER_PROTOCOL)
            .await
        {
            Ok(stream) => stream,
            Err(e) => bail!("Failed to open stream to peer {}: {}", peer_id, e),
        };
        let client = connect_file_transfer_rpc(stream);

        Ok(client)
    }

    pub async fn get_client(&self, name: &str) -> anyhow::Result<ConnectedClient> {
        let Some((peer_id, fc)) = self.clients.lock().get(name).cloned() else {
            bail!("File transfer client with name '{}' not found", name);
        };
        match fc {
            FileClientState::Connected(client) => Ok(client),
            FileClientState::Disconnected => {
                // set the state to connecting
                let start_time = SystemTime::now();
                self.clients.lock().insert(
                    name.to_string(),
                    (peer_id, FileClientState::Connecting(start_time)),
                );
                // try to connect
                let client = match self.connect_client(peer_id).await {
                    Ok(client) => client,
                    Err(e) => {
                        self.clients
                            .lock()
                            .insert(name.to_string(), (peer_id, FileClientState::Disconnected));
                        bail!(
                            "Failed to connect to file transfer client '{}': {}",
                            name,
                            e
                        );
                    }
                };
                let Ok(is_windows) = client.is_windows(context::current()).await else {
                    self.clients
                        .lock()
                        .insert(name.to_string(), (peer_id, FileClientState::Disconnected));
                    bail!("Failed to check if client '{}' is Windows", name);
                };
                let connected_client = ConnectedClient {
                    peer_id: Arc::new(peer_id),
                    is_windows,
                    rpc_client: client,
                };
                // update the client state to connected
                self.clients.lock().insert(
                    name.to_string(),
                    (
                        peer_id,
                        FileClientState::Connected(connected_client.clone()),
                    ),
                );
                Ok(connected_client)
            }
            FileClientState::Connecting(_start_time) => {
                bail!(
                    "File transfer client with name '{}' is currently connecting",
                    name
                );
            }
        }
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
impl FileTransferClientsControl {
    fn is_root_path(mut components: Utf8UnixComponents<'_>) -> bool {
        let Some(first) = components.next() else {
            return true; // empty path is considered root
        };
        if components.next().is_some() {
            return false; // more than one component means it's not root
        }
        first.is_root() || first.is_current() || first.is_empty()
    }

    fn map_rpc_error(&self, rpc_error: RpcError, peer_id: &PeerId) -> fungi_fs::FileSystemError {
        match rpc_error {
            RpcError::Shutdown => {
                log::warn!("Client {peer_id} disconnected");
                let mut lock = self.clients.lock();
                for (_key, (id, state)) in lock.iter_mut() {
                    if id == peer_id {
                        *state = FileClientState::Disconnected;
                    }
                }
                fungi_fs::FileSystemError::ConnectionBroken
            }
            e => fungi_fs::FileSystemError::Other {
                message: e.to_string(),
            },
        }
    }

    async fn extract_client_and_path<'a>(
        &self,
        mut components: Utf8UnixComponents<'a>,
    ) -> anyhow::Result<(ConnectedClient, Utf8UnixComponents<'a>)> {
        let mut client_name = components
            .next()
            .ok_or_else(|| anyhow::anyhow!("No client specified in path"))?;
        if client_name.is_empty() {
            return Err(anyhow::anyhow!("Empty path"));
        }
        // remove the first component if it is root or current
        // "/Test" to "Test"
        if client_name.is_root() || client_name.is_current() {
            client_name = components
                .next()
                .ok_or_else(|| anyhow::anyhow!("No client specified in path"))?;
        }
        let client = self.get_client(client_name.as_str()).await?;
        Ok((client, components))
    }

    pub async fn metadata(&self, path_os_string: &str) -> fungi_fs::Result<fungi_fs::Metadata> {
        let unix_path = convert_string_to_utf8_unix_path_buf(path_os_string).normalize();
        let components: Utf8UnixComponents<'_> = unix_path.components();

        if Self::is_root_path(components.clone()) {
            return Ok(fungi_fs::Metadata {
                is_dir: true,
                is_file: false,
                len: 0,
                modified: Some(std::time::SystemTime::now()),
                created: None,
                accessed: None,
                is_symlink: false,
                gid: 0,
                uid: 0,
                links: 1,
                permissions: 0o555,
                readlink: None,
            });
        }

        let (client, remaining_components) = match self.extract_client_and_path(components).await {
            Ok(result) => result,
            Err(e) => {
                return Err(fungi_fs::FileSystemError::Other {
                    message: e.to_string(),
                });
            }
        };

        client
            .rpc_client
            .metadata(
                context::current(),
                remaining_components.as_str().to_string(),
            )
            .await
            .map_err(|e| self.map_rpc_error(e, &client.peer_id))?
    }

    pub async fn list(&self, path_os_string: &str) -> fungi_fs::Result<Vec<fungi_fs::DirEntry>> {
        let unix_path = convert_string_to_utf8_unix_path_buf(path_os_string).normalize();
        let components: Utf8UnixComponents<'_> = unix_path.components();

        if Self::is_root_path(components.clone()) {
            let clients = self.clients.lock();
            let mut result = Vec::new();

            for client_name in clients.keys() {
                result.push(fungi_fs::DirEntry {
                    name: client_name.clone(),
                    path: PathBuf::from(client_name),
                    metadata: fungi_fs::Metadata {
                        is_dir: true,
                        is_file: false,
                        len: 0,
                        modified: Some(std::time::SystemTime::now()),
                        created: None,
                        accessed: None,
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

        let (client, remaining_components) = match self.extract_client_and_path(components).await {
            Ok(result) => result,
            Err(e) => {
                return Err(fungi_fs::FileSystemError::Other {
                    message: e.to_string(),
                });
            }
        };

        client
            .list(
                context::current(),
                remaining_components.as_str().to_string(),
            )
            .await
            .map_err(|e| self.map_rpc_error(e, &client.peer_id))?
    }

    pub async fn get_chunk(
        &self,
        path_os_string: &str,
        start_pos: u64,
        length: u64,
    ) -> fungi_fs::Result<Vec<u8>> {
        let unix_path = convert_string_to_utf8_unix_path_buf(&path_os_string).normalize();
        let components: Utf8UnixComponents<'_> = unix_path.components();

        if Self::is_root_path(components.clone()) {
            return Err(fungi_fs::FileSystemError::Other {
                message: "Cannot read from root directory".to_string(),
            });
        }

        let (client, remaining_path) = match self.extract_client_and_path(components).await {
            Ok(result) => result,
            Err(e) => {
                return Err(fungi_fs::FileSystemError::Other {
                    message: e.to_string(),
                });
            }
        };

        client
            .get_chunk(
                context::current(),
                remaining_path.as_str().to_string(),
                start_pos,
                length,
            )
            .await
            .map_err(|e| self.map_rpc_error(e, &client.peer_id))?
    }

    pub async fn put(
        &self,
        bytes: Vec<u8>,
        path_os_string: &str,
        start_pos: u64,
    ) -> fungi_fs::Result<u64> {
        let unix_path = convert_string_to_utf8_unix_path_buf(path_os_string).normalize();
        let components: Utf8UnixComponents<'_> = unix_path.components();

        if Self::is_root_path(components.clone()) {
            return Err(fungi_fs::FileSystemError::Other {
                message: "Cannot write to root directory".to_string(),
            });
        }

        let (client, remaining_components) = match self.extract_client_and_path(components).await {
            Ok(result) => result,
            Err(e) => {
                return Err(fungi_fs::FileSystemError::Other {
                    message: e.to_string(),
                });
            }
        };

        client
            .put(
                context::current(),
                bytes,
                remaining_components.as_str().to_string(),
                start_pos,
            )
            .await
            .map_err(|e| self.map_rpc_error(e, &client.peer_id))?
    }

    pub async fn del(&self, path_os_string: &str) -> fungi_fs::Result<()> {
        let unix_path = convert_string_to_utf8_unix_path_buf(path_os_string).normalize();
        let components: Utf8UnixComponents<'_> = unix_path.components();

        if Self::is_root_path(components.clone()) {
            return Err(fungi_fs::FileSystemError::Other {
                message: "Cannot delete root directory".to_string(),
            });
        }

        let (client, mut remaining_components) =
            match self.extract_client_and_path(components).await {
                Ok(result) => result,
                Err(e) => {
                    return Err(fungi_fs::FileSystemError::Other {
                        message: e.to_string(),
                    });
                }
            };

        let path_str = remaining_components.as_str().to_string();

        let Some(first) = remaining_components.next() else {
            return Err(fungi_fs::FileSystemError::Other {
                message: "No path specified for deletion".to_string(),
            });
        };

        if remaining_components.next().is_none() {
            if first.is_root() || first.is_current() || first.is_empty() {
                return Err(fungi_fs::FileSystemError::Other {
                    message: "Cannot delete root or current directory".to_string(),
                });
            }
        }

        client
            .del(context::current(), path_str)
            .await
            .map_err(|e| self.map_rpc_error(e, &client.peer_id))?
    }

    pub async fn rmd(&self, path_os_string: &str) -> fungi_fs::Result<()> {
        let unix_path = convert_string_to_utf8_unix_path_buf(path_os_string).normalize();
        let components: Utf8UnixComponents<'_> = unix_path.components();

        if Self::is_root_path(components.clone()) {
            return Err(fungi_fs::FileSystemError::Other {
                message: "Cannot remove root directory".to_string(),
            });
        }

        let (client, mut remaining_components) =
            match self.extract_client_and_path(components).await {
                Ok(result) => result,
                Err(e) => {
                    return Err(fungi_fs::FileSystemError::Other {
                        message: e.to_string(),
                    });
                }
            };

        let path_str = remaining_components.as_str().to_string();

        let Some(first) = remaining_components.next() else {
            return Err(fungi_fs::FileSystemError::Other {
                message: "No path specified for deletion".to_string(),
            });
        };

        if remaining_components.next().is_none() {
            if first.is_root() || first.is_current() || first.is_empty() {
                return Err(fungi_fs::FileSystemError::Other {
                    message: "Cannot delete root or current directory".to_string(),
                });
            }
        }

        client
            .rmd(context::current(), path_str)
            .await
            .map_err(|e| self.map_rpc_error(e, &client.peer_id))?
    }

    pub async fn mkd(&self, path_os_string: &str) -> fungi_fs::Result<()> {
        let unix_path = convert_string_to_utf8_unix_path_buf(path_os_string).normalize();
        let components: Utf8UnixComponents<'_> = unix_path.components();

        if Self::is_root_path(components.clone()) {
            return Err(fungi_fs::FileSystemError::Other {
                message: "Cannot create directory in root".to_string(),
            });
        }

        let (client, remaining_components) = match self.extract_client_and_path(components).await {
            Ok(result) => result,
            Err(e) => {
                return Err(fungi_fs::FileSystemError::Other {
                    message: e.to_string(),
                });
            }
        };

        client
            .mkd(
                context::current(),
                remaining_components.as_str().to_string(),
            )
            .await
            .map_err(|e| self.map_rpc_error(e, &client.peer_id))?
    }

    pub async fn rename(&self, from_os_string: &str, to_os_string: &str) -> fungi_fs::Result<()> {
        let from_path = convert_string_to_utf8_unix_path_buf(from_os_string).normalize();
        let from_components: Utf8UnixComponents<'_> = from_path.components();

        let to_path = convert_string_to_utf8_unix_path_buf(to_os_string).normalize();
        let to_components: Utf8UnixComponents<'_> = to_path.components();

        if Self::is_root_path(from_components.clone()) || Self::is_root_path(to_components.clone())
        {
            return Err(fungi_fs::FileSystemError::Other {
                message: "Cannot rename root directory".to_string(),
            });
        }

        let from_components_vec: Vec<_> = from_components.clone().collect();
        let to_components_vec: Vec<_> = to_components.clone().collect();
        if from_components_vec.len() <= 1 || to_components_vec.len() <= 1 {
            return Err(fungi_fs::FileSystemError::Other {
                message: "Cannot rename client directories at the top level".to_string(),
            });
        }

        if from_components_vec[0] != to_components_vec[0] {
            return Err(fungi_fs::FileSystemError::Other {
                message: "Cannot rename across different clients".to_string(),
            });
        }

        let (from_client, from_remaining_components) =
            match self.extract_client_and_path(from_components).await {
                Ok(result) => result,
                Err(e) => {
                    return Err(fungi_fs::FileSystemError::Other {
                        message: e.to_string(),
                    });
                }
            };

        let (_to_client, to_remaining_components) =
            match self.extract_client_and_path(to_components).await {
                Ok(result) => result,
                Err(e) => {
                    return Err(fungi_fs::FileSystemError::Other {
                        message: e.to_string(),
                    });
                }
            };

        from_client
            .rename(
                context::current(),
                from_remaining_components.as_str().to_string(),
                to_remaining_components.as_str().to_string(),
            )
            .await
            .map_err(|e| self.map_rpc_error(e, &from_client.peer_id))?
    }

    pub async fn cwd(&self, path_os_string: &str) -> fungi_fs::Result<()> {
        let unix_path = convert_string_to_utf8_unix_path_buf(path_os_string).normalize();
        let components: Utf8UnixComponents<'_> = unix_path.components();

        if Self::is_root_path(components.clone()) {
            return Ok(());
        }

        let (client, remaining_components) = match self.extract_client_and_path(components).await {
            Ok(result) => result,
            Err(e) => {
                return Err(fungi_fs::FileSystemError::Other {
                    message: e.to_string(),
                });
            }
        };

        client
            .cwd(
                context::current(),
                remaining_components.as_str().to_string(),
            )
            .await
            .map_err(|e| self.map_rpc_error(e, &client.peer_id))?
    }
}

pub async fn start_ftp_proxy_service(host: IpAddr, port: u16, client: FileTransferClientsControl) {
    loop {
        let client_cl = client.clone();
        let server = libunftp::ServerBuilder::new(Box::new(move || client_cl.clone()))
            .greeting("Welcome to Fungi FTP proxy")
            .passive_ports(50000..=65535)
            .build()
            .unwrap();

        log::info!("Starting FTP proxy service on port {port}");
        if let Err(e) = server.listen(format!("{host}:{port}")).await {
            log::error!(
                "Failed to start FTP proxy service on port {port}: {e}. Retrying in 5 seconds..."
            );
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

pub async fn start_webdav_proxy_service(
    host: IpAddr,
    port: u16,
    client: FileTransferClientsControl,
) {
    let dav_server = DavHandler::builder()
        .filesystem(Box::new(client))
        .locksystem(FakeLs::new())
        .build_handler();

    let addr = format!("{host}:{port}");
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
                log::error!("Failed serving: {err:?}");
            }
        });
    }
}

pub fn convert_string_to_utf8_unix_path_buf(path: &str) -> Utf8PathBuf<Utf8UnixEncoding> {
    #[cfg(windows)]
    {
        let windows_path = Utf8Path::<typed_path::Utf8WindowsEncoding>::new(path);
        return windows_path.with_encoding::<Utf8UnixEncoding>();
    }
    #[cfg(not(windows))]
    {
        return Utf8Path::<Utf8UnixEncoding>::new(path).with_encoding();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_root_path() {
        // Test various root path representations
        let path = convert_string_to_utf8_unix_path_buf(".");
        assert!(FileTransferClientsControl::is_root_path(path.components()));
        let path = convert_string_to_utf8_unix_path_buf("./");
        assert!(FileTransferClientsControl::is_root_path(path.components()));
        let path = convert_string_to_utf8_unix_path_buf("/");
        assert!(FileTransferClientsControl::is_root_path(path.components()));
        let path = convert_string_to_utf8_unix_path_buf("");
        assert!(FileTransferClientsControl::is_root_path(path.components()));

        // Test non-root paths
        let path = convert_string_to_utf8_unix_path_buf("client1");
        assert!(!FileTransferClientsControl::is_root_path(path.components()));
        let path = convert_string_to_utf8_unix_path_buf("client1/file.txt");
        assert!(!FileTransferClientsControl::is_root_path(path.components()));
        let path = convert_string_to_utf8_unix_path_buf("./client1");
        assert!(!FileTransferClientsControl::is_root_path(path.components()));
        let path = convert_string_to_utf8_unix_path_buf("/client1");
        assert!(!FileTransferClientsControl::is_root_path(path.components()));
        let path = convert_string_to_utf8_unix_path_buf("client1/subdir/file.txt");
        assert!(!FileTransferClientsControl::is_root_path(path.components()));
    }
}
