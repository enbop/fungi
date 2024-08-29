use crate::{listeners::fungi_remote_access::FUNGI_REMOTE_ACCESS_PROTOCOL, DaemonArgs};
use fungi_config::FungiDir;
use fungi_util::ipc::{self, messages::DaemonMessage};
use interprocess::local_socket::tokio::{prelude::*, Stream as IpcStream};
use rand::distributions::{Alphanumeric, DistString};
use std::{io, path::PathBuf};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::compat::FuturesAsyncReadCompatExt;

pub struct FRALocalListener {
    args: DaemonArgs,
    libp2p_stream_control: libp2p_stream::Control,
    listen_task: Option<tokio::task::JoinHandle<()>>,
}

impl FRALocalListener {
    pub fn new(args: DaemonArgs, libp2p_stream_control: libp2p_stream::Control) -> Self {
        Self {
            args,
            libp2p_stream_control,
            listen_task: None,
        }
    }

    pub fn is_started(&self) -> bool {
        self.listen_task.is_some()
    }

    pub async fn start(&mut self) -> io::Result<()> {
        if self.is_started() {
            return Ok(());
        }

        let ipc_listener = ipc::create_ipc_listener(&self.args.fra_ipc_path().to_string_lossy())?;

        let task = tokio::spawn(Self::listen_task(
            self.args.clone(),
            ipc_listener,
            self.libp2p_stream_control.clone(),
        ));
        self.listen_task = Some(task);
        Ok(())
    }

    async fn listen_task(
        args: DaemonArgs,
        ipc_listener: LocalSocketListener,
        libp2p_stream_control: libp2p_stream::Control,
    ) {
        loop {
            let Ok(stream) = ipc_listener.accept().await else {
                log::info!("FRA Local listener is closed");
                break;
            };
            tokio::spawn(Self::handle_local_request_stream(
                args.ipc_dir(),
                stream,
                libp2p_stream_control.clone(),
            ));
        }
    }

    async fn handle_local_request_stream(
        ipc_dir: PathBuf,
        mut stream: IpcStream,
        mut libp2p_stream_control: libp2p_stream::Control,
    ) {
        log::info!("Accepted connection");
        let mut buf = [0; 1024];
        let n = stream.read(&mut buf).await.unwrap();
        let Ok(msg) = bincode::deserialize::<DaemonMessage>(&buf[..n]) else {
            log::info!("Failed to deserialize message");
            return;
        };
        log::info!("Received message: {:?}", msg);
        match msg {
            DaemonMessage::RemoteRequest(remote_peer) => {
                let resp = libp2p_stream_control
                    .open_stream(remote_peer, FUNGI_REMOTE_ACCESS_PROTOCOL)
                    .await
                    .map(|stream| create_forward_ipc_listener(ipc_dir, stream))
                    .unwrap_or_else(|e| Err(format!("Failed to open stream: {:?}", e)));
                let response = DaemonMessage::RemoteResponse(resp);
                let response_bytes = bincode::serialize(&response).unwrap();
                stream.write_all(&response_bytes).await.unwrap();
            }
            _ => {
                log::info!("Unknown message: {:?}", msg);
            }
        }
    }
}

fn create_forward_ipc_listener(
    ipc_dir: PathBuf,
    libp2p_stream: libp2p::Stream,
) -> Result<String, String> {
    let ipc_path = ipc_dir.join(format!(
        "fungi-ra-forward-{}.sock",
        Alphanumeric.sample_string(&mut rand::thread_rng(), 4)
    ));
    let ipc_sock_name = ipc_path.to_string_lossy().to_string();

    let listener = ipc::create_ipc_listener(&ipc_sock_name)
        .map_err(|e| format!("Failed to create IPC listener: {:?}", e))?;

    tokio::spawn(async move {
        // TODO handle error, timeout
        let Ok(mut client) = listener.accept().await else {
            return;
        };
        tokio::io::copy_bidirectional(&mut libp2p_stream.compat(), &mut client)
            .await
            .ok();
        // rm ipc path
        std::fs::remove_file(&ipc_path).ok();
    });

    Ok(ipc_sock_name)
}
