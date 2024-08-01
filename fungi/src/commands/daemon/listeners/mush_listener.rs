use super::WasiListener;
use fungi_util::ipc;
use interprocess::local_socket::tokio::{prelude::*, Stream};
use serde::{Deserialize, Serialize};
use std::{io, path::PathBuf};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub struct MushListener {
    wasi_listener: WasiListener,
    listen_task: Option<tokio::task::JoinHandle<()>>,
}

impl MushListener {
    pub fn new(wasi_listener: WasiListener) -> Self {
        Self {
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
        let task = tokio::spawn(Self::listen_task(listener, self.wasi_listener.clone()));
        self.listen_task = Some(task);
        Ok(())
    }

    async fn listen_task(listener: LocalSocketListener, wasi_listener: WasiListener) {
        loop {
            let Ok(stream) = listener.accept().await else {
                log::info!("Failed to accept connection");
                break;
            };
            tokio::spawn(Self::handle_request_stream(stream, wasi_listener.clone()));
        }
    }

    async fn handle_request_stream(mut stream: Stream, wasi_listener: WasiListener) {
        log::info!("Accepted connection");
        let mut buf = [0; 1024];
        let n = stream.read(&mut buf).await.unwrap();
        let Ok(msg) = bincode::deserialize::<MushMessage>(&buf[..n]) else {
            log::info!("Failed to deserialize message");
            return;
        };
        log::info!("Received message: {:?}", msg);
        match msg {
            MushMessage::InitRequest => {
                let ipc_server_name = wasi_listener.spawn_wasi_process().await;
                let response = MushMessage::InitResponse(ipc_server_name);
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
    InitRequest,
    InitResponse(String),
}
