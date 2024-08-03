mod wasmtime_wasi_impl;
use anyhow::Result;
use fungi_util::ipc;
use interprocess::local_socket::tokio::prelude::*;
use rand::distributions::{Alphanumeric, DistString};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::io::AsyncReadExt;
use wasmtime_wasi_impl::*;

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum IpcMessage {
    Data(String),
}

pub struct WasiProcess {
    ipc_sock_name: String,
    ipc_listener: LocalSocketListener,
    runtime: WasiRuntime,
}

impl WasiProcess {
    pub fn new(ipc_dir: PathBuf, root_dir: PathBuf, bin_dir: PathBuf) -> Result<Self> {
        let ipc_path = ipc_dir.join(format!(
            "fungi-wasi-{}.sock",
            Alphanumeric.sample_string(&mut rand::thread_rng(), 4)
        ));
        let ipc_sock_name = ipc_path.to_string_lossy().to_string();
        let ipc_listener = ipc::create_ipc_listener(&ipc_sock_name)?;
        let runtime = WasiRuntime::new(root_dir, bin_dir)?;
        Ok(Self {
            ipc_sock_name,
            ipc_listener,
            runtime,
        })
    }

    pub fn ipc_sock_name(&self) -> &str {
        &self.ipc_sock_name
    }

    pub async fn start_listen(&mut self) -> Result<()> {
        // TODO only one client at a time
        let (mut rx, _tx) = self.ipc_listener.accept().await?.split();
        let mut buf = [0; 1024];

        // TODO send msg to client
        loop {
            let n = rx.read(&mut buf).await?;
            let IpcMessage::Data(msg) = bincode::deserialize::<IpcMessage>(&buf[..n])?;
            let args = msg.split_whitespace().collect::<Vec<&str>>();
            if args.is_empty() {
                continue; // TODO send msg to client
            }
            self.runtime.spawn(args).await.ok();
        }
    }
}
