use serde::{Deserialize, Serialize};
use std::{io, net::SocketAddr};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

use super::ContainerListener;

pub struct ShellListener {
    container_listener: ContainerListener,
    listen_task: Option<tokio::task::JoinHandle<()>>,
}

impl ShellListener {
    pub fn new(container_listener: ContainerListener) -> Self {
        Self {
            container_listener,
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
        let task = tokio::spawn(Self::listen_task(listener, self.container_listener.clone()));
        self.listen_task = Some(task);
        Ok(())
    }

    async fn listen_task(listener: TcpListener, container_listener: ContainerListener) {
        log::info!("Listening on: {}", listener.local_addr().unwrap());
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                log::info!("Failed to accept connection");
                break;
            };
            tokio::spawn(Self::handle_request_stream(
                stream,
                container_listener.clone(),
            ));
        }
    }

    async fn handle_request_stream(mut stream: TcpStream, container_listener: ContainerListener) {
        log::info!("Accepted connection from: {}", stream.peer_addr().unwrap());
        let mut buf = [0; 1024];
        let n = stream.read(&mut buf).await.unwrap();
        let Ok(msg) = bincode::deserialize::<ShellMessage>(&buf[..n]) else {
            log::info!("Failed to deserialize message");
            return;
        };
        log::info!("Received message: {:?}", msg);
        match msg {
            ShellMessage::InitRequest => {
                let ipc_server_name = container_listener.spawn_wasi_process().await;
                let response = ShellMessage::InitResponse(ipc_server_name);
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
pub enum ShellMessage {
    InitRequest,
    InitResponse(String),
}
