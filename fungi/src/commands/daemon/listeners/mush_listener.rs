use serde::{Deserialize, Serialize};
use std::{io, net::SocketAddr};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

use super::WasiListener;

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

    pub async fn start(&mut self, listen_addr: SocketAddr) -> io::Result<()> {
        if self.is_started() {
            return Ok(());
        }
        let listener = TcpListener::bind(listen_addr).await?;
        let task = tokio::spawn(Self::listen_task(listener, self.wasi_listener.clone()));
        self.listen_task = Some(task);
        Ok(())
    }

    async fn listen_task(listener: TcpListener, wasi_listener: WasiListener) {
        log::info!("Listening on: {}", listener.local_addr().unwrap());
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                log::info!("Failed to accept connection");
                break;
            };
            tokio::spawn(Self::handle_request_stream(
                stream,
                wasi_listener.clone(),
            ));
        }
    }

    async fn handle_request_stream(mut stream: TcpStream, wasi_listener: WasiListener) {
        log::info!("Accepted connection from: {}", stream.peer_addr().unwrap());
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
