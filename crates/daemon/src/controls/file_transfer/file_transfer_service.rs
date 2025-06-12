use std::{
    collections::HashMap,
    io,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use fungi_config::file_transfer::FileTransferService as FileTransferServiceConfig;
use fungi_fs::Result;
use fungi_util::protocols::FUNGI_FILE_TRANSFER_PROTOCOL;
use futures::StreamExt;
use libp2p::PeerId;
use libp2p_stream::Control;
use tarpc::{
    context::Context,
    serde_transport,
    server::{BaseChannel, Channel as _},
    tokio_serde::formats::Bincode,
};
use tokio::task::JoinHandle;
use tokio_util::{codec::LengthDelimitedCodec, compat::FuturesAsyncReadCompatExt as _};

use super::FileTransferRpc as _;

#[derive(Clone)]
pub struct FileTransferRpcService {
    root_dir: Arc<PathBuf>,
    allowed_peers: Arc<Vec<PeerId>>,
    fs: Arc<fungi_fs::FileSystemWrapper>,
}

impl super::FileTransferRpc for FileTransferRpcService {
    async fn metadata(self, _context: Context, path: PathBuf) -> Result<fungi_fs::Metadata> {
        self.fs.metadata(&path).await
    }

    async fn list(self, _context: Context, path: PathBuf) -> Result<Vec<fungi_fs::FileInfo>> {
        self.fs.list(&path).await
    }

    async fn get(self, _context: Context, path: PathBuf, start_pos: u64) -> Result<Vec<u8>> {
        self.fs.get(&path, start_pos).await
    }

    async fn put(
        self,
        _context: Context,
        bytes: Vec<u8>,
        path: PathBuf,
        start_pos: u64,
    ) -> Result<u64> {
        let reader = std::io::Cursor::new(bytes);
        self.fs.put(reader, &path, start_pos).await
    }

    async fn del(self, _context: Context, path: PathBuf) -> Result<()> {
        self.fs.del(&path).await
    }

    async fn rmd(self, _context: Context, path: PathBuf) -> Result<()> {
        self.fs.rmd(&path).await
    }

    async fn mkd(self, _context: Context, path: PathBuf) -> Result<()> {
        self.fs.mkd(&path).await
    }

    async fn rename(self, _context: Context, from: PathBuf, to: PathBuf) -> Result<()> {
        self.fs.rename(&from, &to).await
    }

    async fn cwd(self, _context: Context, path: PathBuf) -> Result<()> {
        self.fs.cwd(&path).await
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
    pub fn new(config: FileTransferServiceConfig) -> io::Result<Self> {
        let fs = fungi_fs::FileSystemWrapper::new(config.shared_root_dir.clone())?;
        Ok(Self {
            fs: Arc::new(fs),
            root_dir: Arc::new(PathBuf::from(config.shared_root_dir)),
            allowed_peers: Arc::new(config.allowed_peers),
        })
    }

    pub async fn listen_from_libp2p_stream(self, mut control: Control) {
        let mut incoming_streams = control.accept(FUNGI_FILE_TRANSFER_PROTOCOL).unwrap();
        log::info!(
            "File Transfer Service listening on protocol: {}",
            FUNGI_FILE_TRANSFER_PROTOCOL
        );
        let codec_builder = LengthDelimitedCodec::builder();

        // TODO: cancel tasks gracefully
        async fn spawn(fut: impl Future<Output = ()> + Send + 'static) {
            tokio::spawn(fut);
        }

        loop {
            let (peer_id, stream) = incoming_streams.next().await.unwrap();
            if !self.allowed_peers.contains(&peer_id) {
                log::warn!("Deny connection from disallowed peer: {}.", peer_id);
                continue;
            }
            log::info!("Accepted connection from peer: {}.", peer_id);

            let framed = codec_builder.new_framed(stream.compat());
            let transport = serde_transport::new(framed, Bincode::default());

            let this = self.clone();
            let fut = BaseChannel::with_defaults(transport)
                .execute(this.serve())
                .for_each(spawn);
            tokio::spawn(fut);
        }
    }
}

#[derive(Clone)]
pub struct FileTransferServiceControl {
    stream_control: Control,
    services: Arc<Mutex<HashMap<PathBuf, JoinHandle<()>>>>,
}

impl FileTransferServiceControl {
    pub fn new(stream_control: Control) -> Self {
        Self {
            stream_control,
            services: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn add_service(&self, config: FileTransferServiceConfig) -> io::Result<()> {
        let mut services = self.services.lock().unwrap();
        if services.contains_key(&config.shared_root_dir) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Service already exists",
            ));
        }

        let service_path = config.shared_root_dir.clone();
        let service = FileTransferRpcService::new(config)?;
        let stream_control = self.stream_control.clone();
        let handle = tokio::spawn(async move {
            service.listen_from_libp2p_stream(stream_control).await;
        });

        services.insert(service_path, handle);
        Ok(())
    }

    pub fn remove_service(&self, path: &PathBuf) {
        let mut services = self.services.lock().unwrap();
        if let Some(handle) = services.remove(path) {
            handle.abort();
        }
    }
}
