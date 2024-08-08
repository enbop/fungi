use super::WasiListener;
use fungi_util::{
    copy_stream,
    ipc::{self, create_ipc_listener},
};
use interprocess::local_socket::tokio::{prelude::*, Stream};
use libp2p::{PeerId, StreamProtocol};
use rand::distributions::{Alphanumeric, DistString};
use serde::{Deserialize, Serialize};
use std::{io, path::PathBuf};
use tokio::io::{AsyncReadExt as TAsyncReadExt, AsyncWriteExt as TAsyncWriteExt};

const MUSHD_PROTOCOL: StreamProtocol = StreamProtocol::new("/fungi/mushd/0.1.0");

pub struct MushListener {
    ipc_dir: PathBuf,
    libp2p_stream_control: libp2p_stream::Control,
    wasi_listener: WasiListener,
    listen_task: Option<tokio::task::JoinHandle<()>>,
}

impl MushListener {
    pub fn new(
        ipc_dir: PathBuf,
        wasi_listener: WasiListener,
        libp2p_stream_control: libp2p_stream::Control,
    ) -> Self {
        Self {
            ipc_dir,
            libp2p_stream_control,
            wasi_listener,
            listen_task: None,
        }
    }

    pub fn is_started(&self) -> bool {
        self.listen_task.is_some()
    }

    pub async fn start(&mut self, ipc_listen_path: PathBuf) -> io::Result<()> {
        if self.is_started() {
            return Ok(());
        }

        let listener = ipc::create_ipc_listener(&ipc_listen_path.to_string_lossy())?;
        log::info!("Listening on: {:?}", ipc_listen_path);
        let task = tokio::spawn(Self::listen_task(
            self.ipc_dir.clone(),
            listener,
            self.wasi_listener.clone(),
            self.libp2p_stream_control.clone(),
        ));
        self.listen_task = Some(task);
        Ok(())
    }

    async fn listen_task(
        ipc_dir: PathBuf,
        listener: LocalSocketListener,
        wasi_listener: WasiListener,
        libp2p_stream_control: libp2p_stream::Control,
    ) {
        loop {
            let Ok(stream) = listener.accept().await else {
                log::info!("Failed to accept connection");
                break;
            };
            tokio::spawn(Self::handle_request_stream(
                ipc_dir.clone(),
                stream,
                wasi_listener.clone(),
                libp2p_stream_control.clone(),
            ));
        }
    }

    async fn handle_request_stream(
        ipc_dir: PathBuf,
        mut stream: Stream,
        wasi_listener: WasiListener,
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
                        .spawn_wasi_process()
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

    let listener = create_ipc_listener(&ipc_sock_name)
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
