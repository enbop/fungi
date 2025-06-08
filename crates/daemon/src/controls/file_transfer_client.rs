use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use fungi_config::{FungiConfig, file_transfer::FileTransferClient as FileTransferClientConfig};
use fungi_swarm::SwarmControl;
use fungi_util::protocols::FUNGI_FILE_TRANSFER_PROTOCOL;
use libp2p::{PeerId, Stream};
use libp2p_stream::Control;
use tarpc::{context, serde_transport, tokio_serde::formats::Bincode};
use tokio::task::JoinHandle;
use tokio_util::{codec::LengthDelimitedCodec, compat::FuturesAsyncReadCompatExt as _};

use crate::controls::FileTransferRpcClient;

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
        start_ftp_proxy_service(config.proxy_ftp_port, client).await;
    }
}

async fn start_ftp_proxy_service(port: u16, client: FileTransferRpcClient) {
    loop {
        let meta = client
            .metadata(context::current(), PathBuf::from("test"))
            .await
            .unwrap();
        // TODO: start proxy ftp server
        println!("DEBUG Meta {:?}", meta);
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}
