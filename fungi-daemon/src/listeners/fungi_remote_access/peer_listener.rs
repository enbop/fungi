use crate::{DaemonArgs, FungiDaemon};
use fungi_config::{FungiConfig, FungiDir};
use fungi_util::{ipc::messages::ForwardStdioMessage, protocols::FUNGI_REMOTE_ACCESS_PROTOCOL};
use futures::{AsyncReadExt as _, AsyncWriteExt as _, StreamExt};
use libp2p::PeerId;
use std::{
    collections::HashMap,
    io,
    path::PathBuf,
    process::Stdio,
    sync::{Arc, Mutex},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::{Child, Command},
};

type AllowedPeers = Arc<Option<Mutex<Vec<PeerId>>>>;
type ChildProcesses = Arc<Mutex<HashMap<PeerId, Vec<Child>>>>;

pub struct FRAPeerListener {
    args: DaemonArgs,
    libp2p_stream_control: libp2p_stream::Control,
    listen_task: Option<tokio::task::JoinHandle<()>>,
    allowed_peers: AllowedPeers,
    child_processes: ChildProcesses,
}

impl FRAPeerListener {
    pub fn new(
        args: DaemonArgs,
        config: FungiConfig,
        libp2p_stream_control: libp2p_stream::Control,
    ) -> Self {
        let allowed_peers = if config.fungi_remote_access.allow_all_peers {
            None
        } else {
            Some(config.fungi_remote_access.allowed_peers.clone())
        };
        Self {
            args,
            libp2p_stream_control,
            listen_task: None,
            allowed_peers: Arc::new(allowed_peers.map(Mutex::new)),
            child_processes: Default::default(),
        }
    }

    pub fn is_started(&self) -> bool {
        self.listen_task.is_some()
    }

    pub async fn start(&mut self) -> io::Result<()> {
        if self.is_started() {
            return Ok(());
        }

        let task = tokio::spawn(Self::listen_task(
            self.args.clone(),
            self.allowed_peers.clone(),
            self.libp2p_stream_control
                .accept(FUNGI_REMOTE_ACCESS_PROTOCOL)
                .unwrap(),
            self.child_processes.clone(),
        ));
        self.listen_task = Some(task);
        Ok(())
    }

    async fn listen_task(
        args: DaemonArgs,
        allowed_peers: AllowedPeers,
        mut incoming_streams: libp2p_stream::IncomingStreams,
        child_processes: ChildProcesses,
    ) {
        loop {
            let Some((peer_id, stream)) = incoming_streams.next().await else {
                log::info!("FRA peer listener is closed");
                break;
            };
            log::info!(
                "FRA peer listener accepted connection from peer: {:?}",
                peer_id
            );
            if let Some(allow_peers) = allowed_peers.as_ref() {
                let allow_peers = allow_peers.lock().unwrap();
                if !allow_peers.contains(&peer_id) {
                    log::info!("Rejecting connection from peer: {:?}", peer_id);
                    continue;
                }
            }
            // TODO handle error
            // TODO child_processes
            tokio::spawn(Self::create_child_process(args.fungi_dir(), stream));
        }
    }

    pub async fn create_child_process(
        fungi_dir: PathBuf,
        mut remote_stream: libp2p::Stream,
    ) -> io::Result<()> {
        let mut child = Command::new(FungiDaemon::get_fungi_bin_path_unchecked())
            .args(["--fungi-dir", fungi_dir.to_str().unwrap()])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let mut stdin = child.stdin.take().unwrap();
        let mut stdout = child.stdout.take().unwrap();
        let mut stderr = child.stderr.take().unwrap();

        // forward stdio to remote
        let mut remote_rx_buf = [0; 1024];
        let mut stdout_buf = [0; 1024];
        let mut stderr_buf = [0; 1024];
        loop {
            tokio::select! {
                remote_msg = remote_stream.read(&mut remote_rx_buf) => {
                    let n = remote_msg?;
                    if n == 0 {
                        break;
                    }
                    let ForwardStdioMessage::Stdin(stdin_data) = bincode::deserialize::<ForwardStdioMessage>(&remote_rx_buf[..n]).map_err(
                        |_| io::Error::new(io::ErrorKind::InvalidData, "Failed to deserialize message"))?
                         else {
                        continue;
                    };
                    stdin.write_all(&stdin_data).await?;
                    stdin.flush().await?;
                },
                stdout_msg = stdout.read(&mut stdout_buf) => {
                    let n = stdout_msg?;
                    if n == 0 {
                        break;
                    }
                    let msg = ForwardStdioMessage::Stdout(stdout_buf[..n].to_vec());
                    let data = bincode::serialize(&msg).expect("Failed to serialize message");
                    remote_stream.write_all(&data).await?;
                    remote_stream.flush().await?;
                },
                stderr_msg = stderr.read(&mut stderr_buf) => {
                    let n = stderr_msg?;
                    if n == 0 {
                        break;
                    }
                    let msg = ForwardStdioMessage::Stderr(stderr_buf[..n].to_vec());
                    let data = bincode::serialize(&msg).expect("Failed to serialize message");
                    remote_stream.write_all(&data).await?;
                    remote_stream.flush().await?;
                },
            }
        }
        Ok(())
    }
}
