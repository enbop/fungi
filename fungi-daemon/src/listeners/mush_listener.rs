use super::WasiListener;
use crate::DaemonArgs;
use fungi_config::{FungiConfig, FungiDir};
use fungi_util::{copy_stream, ipc};
use futures::StreamExt;
use interprocess::local_socket::tokio::{prelude::*, Stream as IpcStream};
use libp2p::{PeerId, StreamProtocol};
use rand::distributions::{Alphanumeric, DistString};
use serde::{Deserialize, Serialize};
use std::{
    io,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tokio::io::{AsyncReadExt as TAsyncReadExt, AsyncWriteExt as TAsyncWriteExt};

const MUSHD_PROTOCOL: StreamProtocol = StreamProtocol::new("/fungi/mushd/0.1.0");

type MushdAllowPeers = Arc<Option<Mutex<Vec<PeerId>>>>;

pub struct MushListener {
    args: DaemonArgs,
    libp2p_stream_control: libp2p_stream::Control,
    wasi_listener: WasiListener,
    listen_task: Option<tokio::task::JoinHandle<()>>,
    mushd_allow_peers: MushdAllowPeers,
}

impl MushListener {
    pub fn new(
        args: DaemonArgs,
        config: FungiConfig,
        wasi_listener: WasiListener,
        libp2p_stream_control: libp2p_stream::Control,
    ) -> Self {
        let mushd_allow_peers = if config.mush_daemon.allow_all_peers {
            None
        } else {
            Some(config.mush_daemon.allow_peers.clone())
        };
        Self {
            args,
            libp2p_stream_control,
            wasi_listener,
            listen_task: None,
            mushd_allow_peers: Arc::new(mushd_allow_peers.map(Mutex::new)),
        }
    }

    pub fn is_started(&self) -> bool {
        self.listen_task.is_some()
    }

    pub async fn start(&mut self) -> io::Result<()> {
        if self.is_started() {
            return Ok(());
        }

        let local_mush_listener =
            ipc::create_ipc_listener(&self.args.mush_ipc_path().to_string_lossy())?;

        let task = tokio::spawn(Self::listen_task(
            self.args.ipc_dir(),
            local_mush_listener,
            self.wasi_listener.clone(),
            self.mushd_allow_peers.clone(),
            self.libp2p_stream_control.accept(MUSHD_PROTOCOL).unwrap(),
            self.libp2p_stream_control.clone(),
        ));
        self.listen_task = Some(task);
        Ok(())
    }

    async fn listen_task(
        ipc_dir: PathBuf,
        local_mush_listener: LocalSocketListener,
        wasi_listener: WasiListener,
        mushd_allow_peers: MushdAllowPeers,
        mut remote_mushd_listener: libp2p_stream::IncomingStreams,
        libp2p_stream_control: libp2p_stream::Control,
    ) {
        loop {
            tokio::select! {
                local_client = local_mush_listener.accept() => {
                    let Ok(stream) = local_client else {
                        log::info!("Local mush listener is closed");
                        break;
                    };
                    tokio::spawn(Self::handle_local_request_stream(
                        ipc_dir.clone(),
                        stream,
                        wasi_listener.clone(),
                        libp2p_stream_control.clone(),
                    ));
                },
                remote_client = remote_mushd_listener.next() => {
                    let Some((peer_id, stream)) = remote_client else {
                        log::info!("Remote mush listener is closed");
                        break;
                    };
                    log::info!("Remote mush listener accepted connection from peer: {:?}", peer_id);
                    if let Some(allow_peers) = mushd_allow_peers.as_ref() {
                        let allow_peers = allow_peers.lock().unwrap();
                        if !allow_peers.contains(&peer_id) {
                            log::info!("Rejecting connection from peer: {:?}", peer_id);
                            continue;
                        }
                    }
                    tokio::spawn(Self::handle_remote_request_stream(peer_id ,stream, wasi_listener.clone()));
                }
            }
        }
    }

    async fn handle_local_request_stream(
        ipc_dir: PathBuf,
        mut stream: IpcStream,
        mut wasi_listener: WasiListener,
        mut libp2p_stream_control: libp2p_stream::Control,
    ) {
        log::info!("Accepted connection");
        let mut buf = [0; 1024];
        let n = stream.read(&mut buf).await.unwrap();
        let Ok(msg) = bincode::deserialize::<MushMessage>(&buf[..n]) else {
            log::info!("Failed to deserialize message");
            return;
        };
        log::info!("Received message: {:?}", msg);
        match msg {
            MushMessage::InitRequest(remote_peer) => {
                let resp = if let Some(remote_peer) = remote_peer {
                    libp2p_stream_control
                        .open_stream(remote_peer, MUSHD_PROTOCOL)
                        .await
                        .map(|stream| create_forward_ipc(ipc_dir, stream))
                        .unwrap_or_else(|e| Err(format!("Failed to open stream: {:?}", e)))
                } else {
                    wasi_listener
                        .spawn_wasi_process(None)
                        .await
                        .map_err(|e| format!("Failed to spawn WASI process: {:?}", e))
                };
                let response = MushMessage::InitResponse(resp);
                let response_bytes = bincode::serialize(&response).unwrap();
                stream.write_all(&response_bytes).await.unwrap();
            }
            _ => {
                log::info!("Unknown message: {:?}", msg);
            }
        }
    }

    async fn handle_remote_request_stream(
        remote_peer_id: PeerId,
        remote_stream: libp2p::Stream,
        mut wasi_listener: WasiListener,
    ) {
        let child_wasi_ipc_name = match wasi_listener.spawn_wasi_process(Some(remote_peer_id)).await
        {
            Ok(name) => name,
            Err(e) => {
                log::error!("Failed to spawn WASI process: {:?}", e);
                return;
            }
        };

        log::info!("Connecting to WASI process {}", child_wasi_ipc_name);
        let Ok(wasi_stream) = ipc::connect_ipc(&child_wasi_ipc_name).await else {
            log::error!("Failed to connect to WASI process");
            return; // TODO handle error and send to remote
        };
        log::info!("Connected to WASI process");
        copy_stream(remote_stream, wasi_stream).await;
    }
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum MushMessage {
    InitRequest(Option<PeerId>),
    InitResponse(Result<String, String>),
}

fn create_forward_ipc(ipc_dir: PathBuf, libp2p_stream: libp2p::Stream) -> Result<String, String> {
    let ipc_path = ipc_dir.join(format!(
        "fungi-mush-forward-{}.sock",
        Alphanumeric.sample_string(&mut rand::thread_rng(), 4)
    ));
    let ipc_sock_name = ipc_path.to_string_lossy().to_string();

    let listener = ipc::create_ipc_listener(&ipc_sock_name)
        .map_err(|e| format!("Failed to create IPC listener: {:?}", e))?;

    tokio::spawn(async move {
        // TODO handle error, timeout
        let Ok(mush_client) = listener.accept().await else {
            return;
        };
        copy_stream(libp2p_stream, mush_client).await;
    });

    Ok(ipc_sock_name)
}
