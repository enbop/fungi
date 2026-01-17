use std::{
    collections::{HashMap, HashSet},
    io,
    path::PathBuf,
    sync::Arc,
};

use fungi_config::file_transfer::FileTransferService as FileTransferServiceConfig;
use fungi_fs::{FileSystem, Result};
use fungi_util::protocols::FUNGI_FILE_TRANSFER_PROTOCOL;
use futures::StreamExt;
use libp2p::PeerId;
use libp2p_stream::{Control, IncomingStreams};
use parking_lot::{Mutex, RwLock};
use tarpc::{
    context::Context,
    serde_transport,
    server::{BaseChannel, Channel as _},
    tokio_serde::formats::Bincode,
};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tokio_util::{codec::LengthDelimitedCodec, compat::FuturesAsyncReadCompatExt as _};

use super::FileTransferRpc as _;

#[derive(Clone)]
pub struct FileTransferRpcService {
    allowed_peers: Arc<RwLock<HashSet<PeerId>>>,
    fs: Arc<FileSystem>,
}

impl super::FileTransferRpc for FileTransferRpcService {
    async fn metadata(self, _context: Context, path: String) -> Result<fungi_fs::Metadata> {
        self.fs.metadata(path).await
    }

    async fn list(self, _context: Context, path: String) -> Result<Vec<fungi_fs::DirEntry>> {
        self.fs.list_dir(path).await
    }

    async fn get_chunk(
        self,
        _context: Context,
        path: String,
        start_pos: u64,
        length: u64,
    ) -> Result<Vec<u8>> {
        self.fs.read_chunk(path, start_pos, length).await
    }

    async fn put(
        self,
        _context: Context,
        bytes: Vec<u8>,
        path: String,
        start_pos: u64,
    ) -> Result<u64> {
        self.fs
            .write_bytes_at_position(path, bytes, start_pos)
            .await
    }

    async fn del(self, _context: Context, path: String) -> Result<()> {
        self.fs.remove_file(path).await
    }

    async fn rmd(self, _context: Context, path: String) -> Result<()> {
        self.fs.remove_dir(path).await
    }

    async fn mkd(self, _context: Context, path: String) -> Result<()> {
        self.fs.create_dir_all(path).await
    }

    async fn rename(self, _context: Context, from: String, to: String) -> Result<()> {
        self.fs.rename(from, to).await
    }

    // TODO: remove this
    async fn cwd(self, _context: Context, _path: String) -> Result<()> {
        // CWD operation doesn't make sense for our filesystem
        // Just return success for compatibility
        Ok(())
    }

    async fn is_windows(self, _context: Context) -> bool {
        #[cfg(target_os = "windows")]
        {
            true
        }
        #[cfg(not(target_os = "windows"))]
        {
            false
        }
    }
}

impl FileTransferRpcService {
    pub fn new(
        config: FileTransferServiceConfig,
        allowed_peers: Arc<RwLock<HashSet<PeerId>>>,
    ) -> io::Result<Self> {
        let fs = FileSystem::new(config.shared_root_dir)?;
        Ok(Self {
            fs: Arc::new(fs),
            allowed_peers,
        })
    }

    pub async fn listen_from_libp2p_stream(
        self,
        mut incoming_streams: IncomingStreams,
        cancellation_token: CancellationToken,
    ) {
        let codec_builder = LengthDelimitedCodec::builder();

        // Store active connection tasks for graceful shutdown
        let active_tasks: Arc<Mutex<Vec<JoinHandle<()>>>> = Arc::new(Mutex::new(Vec::new()));
        let active_tasks_for_cleanup = active_tasks.clone();

        loop {
            tokio::select! {
                stream_result = incoming_streams.next() => {
                    match stream_result {
                        Some((peer_id, stream)) => {
                            if !self.allowed_peers.read().contains(&peer_id) {
                                log::warn!("Deny connection from disallowed peer: {peer_id}.");
                                continue;
                            }
                            log::debug!("Accepted connection from peer: {peer_id}.");

                            let framed = codec_builder.new_framed(stream.compat());
                            let transport = serde_transport::new(framed, Bincode::default());

                            let this = self.clone();
                            let active_tasks_clone = active_tasks.clone();
                            let connection_handle = tokio::spawn(async move {
                                let fut = BaseChannel::with_defaults(transport)
                                    .execute(this.serve())
                                    .for_each(|f| async { tokio::spawn(f); });

                                fut.await;
                                log::debug!("File transfer connection with peer {peer_id} closed");
                            });

                            // Track the connection task and clean up completed ones
                            let mut tasks = active_tasks_clone.lock();
                            tasks.retain(|handle| !handle.is_finished());
                            tasks.push(connection_handle);
                        },
                        None => {
                            log::info!("File Transfer Service stream closed naturally");
                            break;
                        }
                    }
                },
                _ = cancellation_token.cancelled() => {
                    log::info!("File Transfer Service shutdown requested");
                    break;
                }
            }
        }

        // Cleanup: explicitly drop the incoming_streams to release the protocol registration
        log::debug!(
            "File Transfer Service dropping incoming_streams to release protocol registration"
        );
        drop(incoming_streams);

        // Close all active connections
        log::info!("File Transfer Service received shutdown signal, closing active connections...");
        let mut tasks = active_tasks_for_cleanup.lock();
        let task_count = tasks.len();
        if task_count > 0 {
            log::info!("Aborting {task_count} active file transfer connections");
        }
        for handle in tasks.drain(..) {
            handle.abort();
        }
        drop(tasks); // Release the mutex guard

        log::info!("File Transfer Service stopped");
    }
}

#[derive(Clone)]
pub struct FileTransferServiceControl {
    stream_control: Control,
    services: Arc<Mutex<HashMap<PathBuf, CancellationToken>>>,
    incoming_allowed_peers: Arc<RwLock<HashSet<PeerId>>>,
}

impl FileTransferServiceControl {
    pub fn new(
        stream_control: Control,
        incoming_allowed_peers: Arc<RwLock<HashSet<PeerId>>>,
    ) -> Self {
        Self {
            stream_control,
            services: Arc::new(Mutex::new(HashMap::new())),
            incoming_allowed_peers,
        }
    }

    // async is necessary for the tokio::spawn
    pub async fn add_service(&self, config: FileTransferServiceConfig) -> io::Result<()> {
        let mut services = self.services.lock();
        if services.contains_key(&config.shared_root_dir) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Service already exists",
            ));
        }

        let service_path = config.shared_root_dir.clone();
        let service = FileTransferRpcService::new(config, self.incoming_allowed_peers.clone())?;
        let mut stream_control = self.stream_control.clone();
        let cancellation_token = CancellationToken::new();
        let cancellation_token_clone = cancellation_token.clone();

        let incoming_streams = stream_control
            .accept(FUNGI_FILE_TRANSFER_PROTOCOL)
            .map_err(io::Error::other)?;
        log::info!("File Transfer Service listening on protocol: {FUNGI_FILE_TRANSFER_PROTOCOL}");

        tokio::spawn(service.listen_from_libp2p_stream(incoming_streams, cancellation_token_clone));

        services.insert(service_path, cancellation_token);
        Ok(())
    }

    pub fn remove_service(&self, path: &PathBuf) {
        let mut services = self.services.lock();
        if let Some(cancellation_token) = services.remove(path) {
            log::info!("Stopping file transfer service at: {path:?}");
            cancellation_token.cancel();
        }
    }

    pub fn has_service(&self, path: &PathBuf) -> bool {
        self.services.lock().contains_key(path)
    }

    pub fn stop_all(&self) {
        let mut services = self.services.lock();
        for (path, cancellation_token) in services.drain() {
            log::info!("Stopping file transfer service at: {path:?}");
            cancellation_token.cancel();
        }
    }
}
