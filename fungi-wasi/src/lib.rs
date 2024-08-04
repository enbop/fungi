pub(crate) mod stdio_impl;
mod wasmtime_wasi_impl;
use anyhow::Result;
use fungi_util::ipc;
use interprocess::local_socket::tokio::prelude::*;
use rand::distributions::{Alphanumeric, DistString};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use wasmtime_wasi_impl::*;

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum IpcMessage {
    Stdin(Vec<u8>),
    Stdout(Vec<u8>),
    Stderr(Vec<u8>),
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
        let (mut client_rx, mut client_tx) = self.ipc_listener.accept().await?.split();

        let mut buf = [0; 1024];

        loop {
            // send ` # `
            tokio::time::sleep(std::time::Duration::from_nanos(1)).await; // BUG
            client_tx.flush().await?;
            client_tx
                .write_all(&bincode::serialize(&IpcMessage::Stdout(b" # ".to_vec()))?)
                .await?;

            // parse cmd
            let n = client_rx.read(&mut buf).await?;
            let IpcMessage::Stdin(data) = bincode::deserialize::<IpcMessage>(&buf[..n])? else {
                continue;
            };
            let msg = String::from_utf8_lossy(&data);
            let args: Vec<String> = msg.split_whitespace().map(|s| s.to_string()).collect();
            if args.is_empty() {
                continue;
            }

            // run cmd
            let (stdio, mut stdio_handle) = stdio_impl::create_stdio();
            let cmd = match self.runtime.command(args, stdio).await {
                Ok(cmd) => cmd,
                Err(e) => {
                    client_tx
                        .write_all(
                            &bincode::serialize(&IpcMessage::Stderr(format!("{:?}\n", e).into()))
                                .unwrap(),
                        )
                        .await?;
                    continue;
                }
            };

            let forward_stdio = async {
                loop {
                    tokio::select! {
                        data = stdio_handle.stdout_rx.recv() => {
                            let Some(data) = data else {
                                break;
                            };
                            client_tx
                                .write_all(&bincode::serialize(&IpcMessage::Stdout(data.into())).unwrap())
                                .await.unwrap();
                        },
                        data = stdio_handle.stderr_rx.recv() => {
                            let Some(data) = data else {
                                break;
                            };
                            client_tx
                                .write_all(&bincode::serialize(&IpcMessage::Stderr(data.into())).unwrap())
                                .await.unwrap();
                        },
                        n = client_rx.read(&mut buf) => {
                            let Ok(n) = n else {
                                break;
                            };
                            if n == 0 {
                                break;
                            }
                            let IpcMessage::Stdin(data) = bincode::deserialize(&buf[..n]).unwrap()
                            else {
                                break;
                            };
                            stdio_handle.stdin.push_data(data.into());
                        }
                    };
                }
            };

            let (res, _) = tokio::join!(cmd.run(), forward_stdio);
            if let Err(e) = res {
                client_tx
                    .write_all(
                        &bincode::serialize(&IpcMessage::Stderr(e.to_string().into())).unwrap(),
                    )
                    .await?;
            }
        }
    }
}
