use std::{path::PathBuf, sync::Arc};

use fungi_config::file_transfer::FileTransferServer as FileTransferServerConfig;
use fungi_util::protocols::FUNGI_FILE_TRANSFER_PROTOCOL;
use futures::StreamExt;
use libp2p::PeerId;
use libp2p_stream::{Control, IncomingStreams};
use tarpc::{
    context::Context,
    serde_transport,
    server::{BaseChannel, Channel as _},
    tokio_serde::formats::Bincode,
};
use tokio_util::{codec::LengthDelimitedCodec, compat::FuturesAsyncReadCompatExt as _};

use crate::listeners::file_transfer::FileTransferRpc;

#[derive(Clone)]
pub struct FileTransferRpcServer {
    root_dir: Arc<PathBuf>,
    allowed_peers: Arc<Vec<PeerId>>,
    fs: Arc<fungi_fs::FileSystemWrapper>,
}

impl FileTransferRpc for FileTransferRpcServer {
    async fn metadata(self, _context: Context, path: PathBuf) -> fungi_fs::Metadata {
        self.fs.metadata(&path).await
    }
}

impl FileTransferRpcServer {
    pub fn new(config: FileTransferServerConfig) -> Self {
        Self {
            fs: Arc::new(fungi_fs::FileSystemWrapper::new(
                config.shared_root_dir.clone(),
            )),
            root_dir: Arc::new(PathBuf::from(config.shared_root_dir)),
            allowed_peers: Arc::new(config.allowed_peers),
        }
    }

    pub async fn listen_from_libp2p_stream(self, mut control: Control) {
        let mut incoming_streams = control.accept(FUNGI_FILE_TRANSFER_PROTOCOL).unwrap();

        let codec_builder = LengthDelimitedCodec::builder();

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
