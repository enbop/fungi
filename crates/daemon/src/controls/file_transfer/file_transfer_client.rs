use std::{
    collections::HashMap,
    convert::Infallible,
    net::IpAddr,
    ops::{Deref, DerefMut},
    path::{Component, Path, PathBuf},
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
use tarpc::{context, serde_transport, tokio_serde::formats::Bincode};
use tokio::net::TcpListener;
use tokio_util::{codec::LengthDelimitedCodec, compat::FuturesAsyncReadCompatExt as _};

use crate::controls::file_transfer::FileTransferRpcClient;

#[derive(Debug, Clone)]
struct ConnectedClient {
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
}

impl std::fmt::Debug for FileTransferClientsControl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // only print name "FileTransferClientControl"
        f.debug_struct("FileTransferClientControl").finish()
    }
}

impl FileTransferClientsControl {
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
        log::info!("Removing file transfer client with peer_id: {}", peer_id);
        self.clients.lock().retain(|_, (id, _)| id != peer_id);
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

    async fn get_client(&self, name: &str) -> anyhow::Result<ConnectedClient> {
        let Some((peer_id, fc)) = self.clients.lock().get(name).cloned() else {
            bail!("File transfer client with name '{}' not found", name);
        };
        match fc {
            FileClientState::Connected(client) => {
                return Ok(client);
            }
            FileClientState::Disconnected => {
                // set the state to connecting
                let start_time = SystemTime::now();
                self.clients.lock().insert(
                    name.to_string(),
                    (peer_id.clone(), FileClientState::Connecting(start_time)),
                );
                // try to connect
                let client = match self.connect_client(peer_id.clone()).await {
                    Ok(client) => client,
                    Err(e) => {
                        self.clients.lock().insert(
                            name.to_string(),
                            (peer_id.clone(), FileClientState::Disconnected),
                        );
                        bail!(
                            "Failed to connect to file transfer client '{}': {}",
                            name,
                            e
                        );
                    }
                };
                let Ok(is_windows) = client.is_windows(context::current()).await else {
                    self.clients.lock().insert(
                        name.to_string(),
                        (peer_id.clone(), FileClientState::Disconnected),
                    );
                    bail!("Failed to check if client '{}' is Windows", name);
                };
                let connected_client = ConnectedClient {
                    peer_id: Arc::new(peer_id.clone()),
                    is_windows,
                    rpc_client: client,
                };
                // update the client state to connected
                self.clients.lock().insert(
                    name.to_string(),
                    (
                        peer_id.clone(),
                        FileClientState::Connected(connected_client.clone()),
                    ),
                );
                return Ok(connected_client);
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
    fn is_root_path(path: &Path) -> bool {
        let components: Vec<_> = path.components().collect();
        matches!(
            components.as_slice(),
            [] | [Component::CurDir]
                | [Component::RootDir]
                | [Component::CurDir, Component::RootDir]
        )
    }

    fn map_rpc_error(&self, peer_id: &PeerId) -> fungi_fs::FileSystemError {
        log::warn!("Client {} disconnected", peer_id);
        let mut lock = self.clients.lock();
        for (_key, (id, state)) in lock.iter_mut() {
            if id == peer_id {
                *state = FileClientState::Disconnected;
            }
        }
        fungi_fs::FileSystemError::ConnectionBroken
    }

    async fn extract_client_and_path(
        &self,
        path: PathBuf,
    ) -> anyhow::Result<(ConnectedClient, PathBuf)> {
        let components: Vec<Component> = path.components().collect();

        let meaningful_components: Vec<&str> = components
            .iter()
            .filter_map(|c| match c {
                Component::Normal(name) => name.to_str(),
                _ => None,
            })
            .collect();

        if meaningful_components.is_empty() {
            bail!("Cannot perform operation on root directory. Please specify a client directory.");
        }

        let client_name = meaningful_components[0];
        let client = self.get_client(client_name).await?;

        let remaining_path = if meaningful_components.len() > 1 {
            meaningful_components[1..].iter().collect::<PathBuf>()
        } else {
            PathBuf::from(".")
        };

        log::debug!(
            "Extracted client: {} and remaining path: {}",
            client_name,
            remaining_path.display()
        );

        Ok((client, remaining_path))
    }

    pub async fn metadata(&self, path: PathBuf) -> fungi_fs::Result<fungi_fs::Metadata> {
        if Self::is_root_path(&path) {
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

        let (client, remaining_path) = match self.extract_client_and_path(path).await {
            Ok(result) => result,
            Err(e) => {
                return Err(fungi_fs::FileSystemError::Other {
                    message: e.to_string(),
                });
            }
        };

        client
            .rpc_client
            .metadata(context::current(), remaining_path)
            .await
            .map_err(|_| self.map_rpc_error(&client.peer_id))?
    }

    pub async fn list(&self, path: PathBuf) -> fungi_fs::Result<Vec<fungi_fs::DirEntry>> {
        if Self::is_root_path(&path) {
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

        let (client, remaining_path) = match self.extract_client_and_path(path).await {
            Ok(result) => result,
            Err(e) => {
                return Err(fungi_fs::FileSystemError::Other {
                    message: e.to_string(),
                });
            }
        };

        client
            .list(context::current(), remaining_path)
            .await
            .map_err(|_| self.map_rpc_error(&client.peer_id))?
    }

    pub async fn get(&self, path: PathBuf, start_pos: u64) -> fungi_fs::Result<Vec<u8>> {
        if Self::is_root_path(&path) {
            return Err(fungi_fs::FileSystemError::Other {
                message: "Cannot read from root directory".to_string(),
            });
        }

        let (client, remaining_path) = match self.extract_client_and_path(path).await {
            Ok(result) => result,
            Err(e) => {
                return Err(fungi_fs::FileSystemError::Other {
                    message: e.to_string(),
                });
            }
        };

        client
            .get(context::current(), remaining_path, start_pos)
            .await
            .map_err(|_| self.map_rpc_error(&client.peer_id))?
    }

    pub async fn put(
        &self,
        bytes: Vec<u8>,
        path: PathBuf,
        start_pos: u64,
    ) -> fungi_fs::Result<u64> {
        if Self::is_root_path(&path) {
            return Err(fungi_fs::FileSystemError::Other {
                message: "Cannot write to root directory".to_string(),
            });
        }

        let (client, remaining_path) = match self.extract_client_and_path(path).await {
            Ok(result) => result,
            Err(e) => {
                return Err(fungi_fs::FileSystemError::Other {
                    message: e.to_string(),
                });
            }
        };

        client
            .put(context::current(), bytes, remaining_path, start_pos)
            .await
            .map_err(|_| self.map_rpc_error(&client.peer_id))?
    }

    pub async fn del(&self, path: PathBuf) -> fungi_fs::Result<()> {
        if Self::is_root_path(&path) {
            return Err(fungi_fs::FileSystemError::Other {
                message: "Cannot delete root directory".to_string(),
            });
        }

        let (client, remaining_path) = match self.extract_client_and_path(path).await {
            Ok(result) => result,
            Err(e) => {
                return Err(fungi_fs::FileSystemError::Other {
                    message: e.to_string(),
                });
            }
        };

        if remaining_path.to_string_lossy() == "." {
            return Err(fungi_fs::FileSystemError::Other {
                message: "Cannot delete client root directory".to_string(),
            });
        }

        client
            .del(context::current(), remaining_path)
            .await
            .map_err(|_| self.map_rpc_error(&client.peer_id))?
    }

    pub async fn rmd(&self, path: PathBuf) -> fungi_fs::Result<()> {
        if Self::is_root_path(&path) {
            return Err(fungi_fs::FileSystemError::Other {
                message: "Cannot remove root directory".to_string(),
            });
        }

        let (client, remaining_path) = match self.extract_client_and_path(path).await {
            Ok(result) => result,
            Err(e) => {
                return Err(fungi_fs::FileSystemError::Other {
                    message: e.to_string(),
                });
            }
        };

        if remaining_path.to_string_lossy() == "." {
            return Err(fungi_fs::FileSystemError::Other {
                message: "Cannot remove client root directory".to_string(),
            });
        }

        client
            .rmd(context::current(), remaining_path)
            .await
            .map_err(|_| self.map_rpc_error(&client.peer_id))?
    }

    pub async fn mkd(&self, path: PathBuf) -> fungi_fs::Result<()> {
        if Self::is_root_path(&path) {
            return Err(fungi_fs::FileSystemError::Other {
                message: "Cannot create directory in root".to_string(),
            });
        }

        let (client, remaining_path) = match self.extract_client_and_path(path).await {
            Ok(result) => result,
            Err(e) => {
                return Err(fungi_fs::FileSystemError::Other {
                    message: e.to_string(),
                });
            }
        };

        client
            .mkd(context::current(), remaining_path)
            .await
            .map_err(|_| self.map_rpc_error(&client.peer_id))?
    }

    pub async fn rename(&self, from: PathBuf, to: PathBuf) -> fungi_fs::Result<()> {
        if Self::is_root_path(&from) || Self::is_root_path(&to) {
            return Err(fungi_fs::FileSystemError::Other {
                message: "Cannot rename root directory".to_string(),
            });
        }

        let from_components: Vec<_> = from
            .components()
            .filter_map(|c| match c {
                Component::Normal(name) => name.to_str(),
                _ => None,
            })
            .collect();

        let to_components: Vec<_> = to
            .components()
            .filter_map(|c| match c {
                Component::Normal(name) => name.to_str(),
                _ => None,
            })
            .collect();

        if from_components.len() <= 1 || to_components.len() <= 1 {
            return Err(fungi_fs::FileSystemError::Other {
                message: "Cannot rename client directories at the top level".to_string(),
            });
        }

        if from_components[0] != to_components[0] {
            return Err(fungi_fs::FileSystemError::Other {
                message: "Cannot rename across different clients".to_string(),
            });
        }

        let (from_client, from_remaining_path) =
            match self.extract_client_and_path(from.clone()).await {
                Ok(result) => result,
                Err(e) => {
                    return Err(fungi_fs::FileSystemError::Other {
                        message: e.to_string(),
                    });
                }
            };

        let (_to_client, to_remaining_path) = match self.extract_client_and_path(to.clone()).await {
            Ok(result) => result,
            Err(e) => {
                return Err(fungi_fs::FileSystemError::Other {
                    message: e.to_string(),
                });
            }
        };

        if from_remaining_path.to_string_lossy() == "." {
            return Err(fungi_fs::FileSystemError::Other {
                message: "Cannot rename client root directory".to_string(),
            });
        }

        from_client
            .rename(context::current(), from_remaining_path, to_remaining_path)
            .await
            .map_err(|_| self.map_rpc_error(&from_client.peer_id))?
    }

    pub async fn cwd(&self, path: PathBuf) -> fungi_fs::Result<()> {
        if Self::is_root_path(&path) {
            return Ok(());
        }

        let (client, remaining_path) = match self.extract_client_and_path(path).await {
            Ok(result) => result,
            Err(e) => {
                return Err(fungi_fs::FileSystemError::Other {
                    message: e.to_string(),
                });
            }
        };

        client
            .cwd(context::current(), remaining_path)
            .await
            .map_err(|_| self.map_rpc_error(&client.peer_id))?
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
    client: FileTransferClientsControl,
) {
    let dav_server = DavHandler::builder()
        .filesystem(Box::new(client))
        // TODO macos finder needs the locking support. https://sabre.io/dav/clients/finder/
        .locksystem(FakeLs::new())
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_is_root_path() {
        // Test various root path representations
        assert!(FileTransferClientsControl::is_root_path(Path::new(".")));
        assert!(FileTransferClientsControl::is_root_path(Path::new("./")));
        assert!(FileTransferClientsControl::is_root_path(Path::new("/")));
        assert!(FileTransferClientsControl::is_root_path(Path::new("")));

        // Test non-root paths
        assert!(!FileTransferClientsControl::is_root_path(Path::new(
            "client1"
        )));
        assert!(!FileTransferClientsControl::is_root_path(Path::new(
            "client1/file.txt"
        )));
        assert!(!FileTransferClientsControl::is_root_path(Path::new(
            "./client1"
        )));
        assert!(!FileTransferClientsControl::is_root_path(Path::new(
            "/client1"
        )));
        assert!(!FileTransferClientsControl::is_root_path(Path::new(
            "client1/subdir/file.txt"
        )));
    }

    #[test]
    fn test_path_component_extraction() {
        // Test extracting components from various path formats
        let test_cases = vec![
            ("client1", vec!["client1"]),
            ("client1/file.txt", vec!["client1", "file.txt"]),
            ("./client1/file.txt", vec!["client1", "file.txt"]),
            ("/client1/file.txt", vec!["client1", "file.txt"]),
            (
                "client1/subdir/file.txt",
                vec!["client1", "subdir", "file.txt"],
            ),
            ("client1/subdir/", vec!["client1", "subdir"]),
        ];

        for (path_str, expected) in test_cases {
            let path = PathBuf::from(path_str);
            let components: Vec<&str> = path
                .components()
                .filter_map(|c| match c {
                    Component::Normal(name) => name.to_str(),
                    _ => None,
                })
                .collect();

            assert_eq!(components, expected, "Failed for path: {}", path_str);
        }
    }

    #[test]
    fn test_path_component_extraction_empty_paths() {
        let empty_paths = vec!["", ".", "./", "/"];

        for path_str in empty_paths {
            let path = PathBuf::from(path_str);
            let components: Vec<&str> = path
                .components()
                .filter_map(|c| match c {
                    Component::Normal(name) => name.to_str(),
                    _ => None,
                })
                .collect();

            assert!(
                components.is_empty(),
                "Path '{}' should have no meaningful components",
                path_str
            );
        }
    }

    #[test]
    fn test_remaining_path_construction() {
        // Test remaining path construction logic
        let test_cases = vec![
            (vec!["client1"], "."),
            (vec!["client1", "file.txt"], "file.txt"),
            (vec!["client1", "subdir", "file.txt"], if cfg!(windows) { "subdir\\file.txt" } else { "subdir/file.txt" }),
            (
                vec!["client1", "subdir", "nested", "file.txt"],
                if cfg!(windows) { "subdir\\nested\\file.txt" } else { "subdir/nested/file.txt" },
            ),
        ];

        for (components, expected) in test_cases {
            let remaining_path = if components.len() > 1 {
                components[1..].iter().collect::<PathBuf>()
            } else {
                PathBuf::from(".")
            };

            assert_eq!(
                remaining_path.to_string_lossy(),
                expected,
                "Failed for components: {:?}",
                components
            );
        }
    }

    #[test]
    fn test_cross_platform_path_handling() {
        // Test that our component-based approach works consistently across platforms
        #[cfg(windows)]
        {
            let path = PathBuf::from(r"client1\subdir\file.txt");
            let components: Vec<&str> = path
                .components()
                .filter_map(|c| match c {
                    Component::Normal(name) => name.to_str(),
                    _ => None,
                })
                .collect();
            assert_eq!(components, vec!["client1", "subdir", "file.txt"]);
        }

        #[cfg(unix)]
        {
            let path = PathBuf::from("client1/subdir/file.txt");
            let components: Vec<&str> = path
                .components()
                .filter_map(|c| match c {
                    Component::Normal(name) => name.to_str(),
                    _ => None,
                })
                .collect();
            assert_eq!(components, vec!["client1", "subdir", "file.txt"]);
        }
    }

    #[test]
    fn test_special_path_components() {
        // Test handling of special path components
        let path = PathBuf::from("./client1/../client2/./file.txt");
        let components: Vec<Component> = path.components().collect();

        // Check that we can identify different component types
        let normal_components: Vec<&str> = components
            .iter()
            .filter_map(|c| match c {
                Component::Normal(name) => name.to_str(),
                _ => None,
            })
            .collect();

        // Should only extract "client1", "client2", "file.txt", ignoring "." and ".."
        assert_eq!(normal_components, vec!["client1", "client2", "file.txt"]);
    }

    #[test]
    fn test_rename_path_validation_logic() {
        // Test the logic used in rename method for path validation
        let test_cases = vec![
            ("client1/file1.txt", "client1/file2.txt", true), // Same client, should be valid
            ("client1/dir1/file.txt", "client1/dir2/file.txt", true), // Same client, different dirs
            ("client1/file.txt", "client2/file.txt", false), // Different clients, should be invalid
        ];

        for (from_path, to_path, should_be_valid) in test_cases {
            let from = PathBuf::from(from_path);
            let to = PathBuf::from(to_path);

            let from_components: Vec<&str> = from
                .components()
                .filter_map(|c| match c {
                    Component::Normal(name) => name.to_str(),
                    _ => None,
                })
                .collect();

            let to_components: Vec<&str> = to
                .components()
                .filter_map(|c| match c {
                    Component::Normal(name) => name.to_str(),
                    _ => None,
                })
                .collect();

            let is_same_client = from_components.first() == to_components.first()
                && from_components.len() > 1
                && to_components.len() > 1;

            assert_eq!(
                is_same_client, should_be_valid,
                "Validation failed for {} -> {}",
                from_path, to_path
            );
        }
    }

    #[test]
    fn test_client_directory_detection() {
        // Test detection of top-level client directory operations
        let test_cases = vec![
            ("client1", true),           // Top-level client directory
            ("client1/", true),          // Top-level client directory with trailing slash
            ("client1/file.txt", false), // File within client directory
            ("client1/subdir", false),   // Subdirectory within client
        ];

        for (path_str, is_top_level) in test_cases {
            let path = PathBuf::from(path_str);
            let components: Vec<&str> = path
                .components()
                .filter_map(|c| match c {
                    Component::Normal(name) => name.to_str(),
                    _ => None,
                })
                .collect();

            let is_client_dir = components.len() == 1;
            assert_eq!(
                is_client_dir, is_top_level,
                "Client directory detection failed for: {}",
                path_str
            );
        }
    }

    #[test]
    fn test_windows_path_separators() {
        // Test handling of Windows-style path separators
        // Note: This test will behave differently on Windows vs Unix systems
        let path = PathBuf::from("client1\\subdir\\file.txt");
        let components: Vec<&str> = path
            .components()
            .filter_map(|c| match c {
                Component::Normal(name) => name.to_str(),
                _ => None,
            })
            .collect();

        // On Windows, this should split into separate components
        // On Unix, the backslashes are treated as part of the filename
        #[cfg(windows)]
        assert_eq!(components, vec!["client1", "subdir", "file.txt"]);

        #[cfg(unix)]
        assert_eq!(components, vec!["client1\\subdir\\file.txt"]);
    }

    #[test]
    fn test_path_normalization() {
        // Test that our approach handles various path formats consistently
        let path_variants = vec![
            "client1/file.txt",
            "./client1/file.txt",
            "/client1/file.txt",
            "client1//file.txt", // Double slash
        ];

        for path_str in path_variants {
            let path = PathBuf::from(path_str);
            let components: Vec<&str> = path
                .components()
                .filter_map(|c| match c {
                    Component::Normal(name) => name.to_str(),
                    _ => None,
                })
                .collect();

            // All should result in the same components
            assert_eq!(
                components,
                vec!["client1", "file.txt"],
                "Path normalization failed for: {}",
                path_str
            );
        }
    }

    #[test]
    fn test_windows_specific_root_paths() {
        // Test Windows-specific root path representations
        let windows_root_paths = vec![
            "\\",      // Windows root
            ".\\",     // Current directory with Windows separator
            "\\\\",    // UNC path start (treated as root in our context)
        ];

        for path_str in windows_root_paths {
            let path = PathBuf::from(path_str);
            let is_root = FileTransferClientsControl::is_root_path(&path);
            
            // On Windows, these should be treated as root paths
            // On Unix, backslashes are treated as regular characters
            #[cfg(windows)]
            assert!(is_root, "Windows path '{}' should be recognized as root", path_str);
            
            // On Unix, we might not recognize these as root, which is acceptable
            // since they would be invalid paths anyway
            #[cfg(unix)]
            {
                // Just ensure the function doesn't panic
                let _ = is_root;
            }
        }
    }

    #[test]
    fn test_windows_vs_unix_path_differences() {
        // Test paths that behave differently on Windows vs Unix
        let test_cases = vec![
            ("client1\\file.txt", vec!["client1", "file.txt"]), // Should work on Windows
            (".\\client1\\file.txt", vec!["client1", "file.txt"]), // Windows current dir
            ("\\client1\\file.txt", vec!["client1", "file.txt"]), // Windows absolute
        ];

        for (path_str, _expected_on_windows) in test_cases {
            let path = PathBuf::from(path_str);
            let components: Vec<&str> = path
                .components()
                .filter_map(|c| match c {
                    Component::Normal(name) => name.to_str(),
                    _ => None,
                })
                .collect();

            #[cfg(windows)]
            assert_eq!(
                components, _expected_on_windows,
                "Windows path parsing failed for: {}",
                path_str
            );

            #[cfg(unix)]
            {
                // On Unix, backslashes are part of the filename
                // So "client1\file.txt" becomes a single component "client1\file.txt"
                match path_str {
                    "client1\\file.txt" => {
                        assert_eq!(components, vec!["client1\\file.txt"]);
                    }
                    ".\\client1\\file.txt" => {
                        assert_eq!(components, vec![".\\client1\\file.txt"]);
                    }
                    "\\client1\\file.txt" => {
                        assert_eq!(components, vec!["\\client1\\file.txt"]);
                    }
                    _ => {}
                }
            }
        }
    }

    #[test]
    fn test_mixed_path_separators() {
        // Test paths with mixed separators (both / and \)
        let mixed_paths = vec![
            "client1/subdir\\file.txt",
            "client1\\subdir/file.txt",
            "./client1\\file.txt",
            ".\\client1/file.txt",
        ];

        for path_str in mixed_paths {
            let path = PathBuf::from(path_str);
            let components: Vec<&str> = path
                .components()
                .filter_map(|c| match c {
                    Component::Normal(name) => name.to_str(),
                    _ => None,
                })
                .collect();

            // Ensure we always extract at least one component
            assert!(!components.is_empty(), "Should extract at least one component from: {}", path_str);
            
            // On Windows, should properly separate components; on Unix, backslashes are part of filename
            let first_component = components[0];
            
            #[cfg(windows)]
            {
                // On Windows, should extract clean client name
                assert!(
                    first_component == "client1",
                    "Windows: First component should be client1, got: {} from path: {}",
                    first_component, path_str
                );
            }
            
            #[cfg(unix)]
            {
                // On Unix, backslashes are treated as part of the filename
                // So we just check that we have meaningful content
                assert!(
                    first_component.contains("client1"),
                    "Unix: First component should contain client1, got: {} from path: {}",
                    first_component, path_str
                );
            }
        }
    }

    #[test]
    fn test_edge_case_paths() {
        // Test various edge cases that might break path parsing
        let edge_cases = vec![
            "",           // Empty path
            ".",          // Current directory
            "..",         // Parent directory
            "/",          // Unix root
            "\\",         // Windows root (on Unix this is just a backslash filename)
            "./",         // Current directory with trailing slash
            "../",        // Parent directory with trailing slash
            "client1/",   // Client with trailing slash
            "client1\\",  // Client with Windows trailing slash
            "//client1",  // Double slash prefix
            "\\\\client1", // Windows UNC-style path
        ];

        for path_str in edge_cases {
            let path = PathBuf::from(path_str);
            
            // Test that is_root_path doesn't panic
            let is_root = FileTransferClientsControl::is_root_path(&path);
            
            // Test that component extraction doesn't panic
            let components: Vec<&str> = path
                .components()
                .filter_map(|c| match c {
                    Component::Normal(name) => name.to_str(),
                    _ => None,
                })
                .collect();
            
            // For paths that should be root, verify they're detected correctly
            match path_str {
                "" | "." | "./" | "/" => {
                    assert!(is_root, "Path '{}' should be detected as root", path_str);
                    assert!(components.is_empty(), "Root path '{}' should have no normal components", path_str);
                }
                _ => {
                    // Non-root paths should have components or be properly handled
                    if !is_root {
                        // If not root, should either have components or be a special case
                        let has_meaningful_content = !components.is_empty() || 
                            path_str.contains("..") ||  // Parent directory references
                            (cfg!(unix) && path_str.contains("\\")); // Backslashes on Unix
                        
                        assert!(
                            has_meaningful_content || path_str.len() <= 2,
                            "Non-root path '{}' should have meaningful content or be very short",
                            path_str
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn test_unicode_and_special_characters() {
        // Test paths with Unicode and special characters
        let special_paths = vec![
            "客户端1/文件.txt",           // Chinese characters
            "client1/файл.txt",        // Cyrillic characters
            "client1/file with spaces.txt", // Spaces
            "client1/file-with-dashes.txt", // Dashes
            "client1/file_with_underscores.txt", // Underscores
            "client1/file.with.dots.txt",   // Multiple dots
        ];

        for path_str in special_paths {
            let path = PathBuf::from(path_str);
            let components: Vec<&str> = path
                .components()
                .filter_map(|c| match c {
                    Component::Normal(name) => name.to_str(),
                    _ => None,
                })
                .collect();

            // Should always have at least 2 components (client and file)
            assert!(
                components.len() >= 2,
                "Special character path should have at least 2 components: {}",
                path_str
            );

            // Verify we can extract the client name correctly
            let client_name = components[0];
            assert!(
                !client_name.is_empty(),
                "Client name should not be empty for path: {}",
                path_str
            );
        }
    }
}
