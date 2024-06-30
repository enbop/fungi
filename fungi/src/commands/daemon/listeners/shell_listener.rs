use serde::{Deserialize, Serialize};
use std::{io, net::SocketAddr};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
#[derive(Default)]
pub struct ShellListener {
    listen_task: Option<tokio::task::JoinHandle<()>>,
}

impl ShellListener {
    pub fn is_started(&self) -> bool {
        self.listen_task.is_some()
    }

    pub async fn start(&mut self, listen_addr: SocketAddr) -> io::Result<()> {
        if self.is_started() {
            return Ok(());
        }
        let listener = TcpListener::bind(listen_addr).await?;
        let task = tokio::spawn(Self::listen_task(listener));
        Ok(())
    }

    async fn listen_task(listener: TcpListener) {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                log::info!("Failed to accept connection");
                break;
            };
            tokio::spawn(Self::handle_request_stream(stream));
        }
    }

    async fn handle_request_stream(mut stream: TcpStream) {
        let mut buf = [0; 1024];
        let n = stream.read(&mut buf).await.unwrap();
        let Ok(msg) = bincode::deserialize::<ShellMessage>(&buf[..n]) else {
            log::info!("Failed to deserialize message");
            return;
        };
        match msg {
            ShellMessage::InitRequest => {
                let response = ShellMessage::InitResponse("Hello, World!".to_string());
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
