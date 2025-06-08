use std::{path::PathBuf, time::Duration};

use fungi_config::{FungiConfig, file_transfer::FileTransferClient as FileTransferClientConfig};
use fungi_util::protocols::FUNGI_FILE_TRANSFER_PROTOCOL;
use libp2p::Stream;
use libp2p_stream::Control;
use tarpc::{context, serde_transport, tokio_serde::formats::Bincode};
use tokio_util::{codec::LengthDelimitedCodec, compat::FuturesAsyncReadCompatExt as _};

use crate::controls::FileTransferRpcClient;

// TODO state management
pub struct FileTransferLocalListener {
    client_infos: Vec<FileTransferClientConfig>,
}

fn connect_file_transfer_rpc(stream: Stream) -> FileTransferRpcClient {
    let codec_builder = LengthDelimitedCodec::builder();
    let transport = serde_transport::new(
        codec_builder.new_framed(stream.compat()),
        Bincode::default(),
    );
    FileTransferRpcClient::new(Default::default(), transport).spawn()
}

impl FileTransferLocalListener {
    pub async fn start(
        client_infos: Vec<FileTransferClientConfig>,
        mut stream_control: Control,
    ) -> Self {
        tokio::time::sleep(Duration::from_secs(1)).await; // TODO: remove this sleep
        for c in &client_infos {
            let stream = stream_control
                .open_stream(c.target_peer, FUNGI_FILE_TRANSFER_PROTOCOL)
                .await
                .unwrap();
            let client = connect_file_transfer_rpc(stream);
            let meta = client
                .metadata(context::current(), PathBuf::from("test"))
                .await
                .unwrap();
            // TODO: start proxy ftp server
            println!("DEBUG Connected to {}: {:?}", c.target_peer, meta);
        }

        Self { client_infos }
    }

    pub async fn start_proxy_ftp(&mut self) {
        todo!()
    }
}
