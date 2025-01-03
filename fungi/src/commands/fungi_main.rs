use super::FungiArgs;
use fungi_config::FungiDir;
use fungi_daemon::listeners::FungiDaemonRpcClient;
use fungi_util::ipc::{
    self,
    messages::{DaemonMessage, ForwardStdioMessage},
};
use interprocess::local_socket::{
    tokio::{RecvHalf, SendHalf, Stream},
    traits::tokio::Stream as TraitStream,
};
use libp2p::PeerId;
use std::{io::Write, process::exit};
use tarpc::{
    serde_transport, tokio_serde::formats::Bincode, tokio_util::codec::LengthDelimitedCodec,
};
use tokio::io::{self, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

pub async fn run(args: FungiArgs) {
    fungi_config::init(&args).unwrap();
    println!("Connecting to fungi daemon");

    // TODO remove fra local listener, use the daemon rpc to send control messages
    let fra_stream = ipc::connect_ipc(&args.fra_ipc_path().to_string_lossy()).await;

    let daemon_rpc_stream = ipc::connect_ipc(&args.daemon_rpc_path().to_string_lossy()).await;
    if daemon_rpc_stream.is_err() {
        println!("Failed to connect to fungi daemon, is it running?");
    };

    let daemon_rpc_client = daemon_rpc_stream
        .ok()
        .map(|stream| connect_daemon_rpc(stream));

    if let Some(remote_peer) = args.peer {
        let Ok(daemon_ipc_stream) = fra_stream else {
            eprintln!("You cannot connect to a remote peer without a running daemon, run `fungi daemon` first");
            exit(1);
        };
        println!("Connecting to remote peer: {}", remote_peer);
        run_remote(daemon_ipc_stream, remote_peer).await;
    } else {
        println!("Starting Fungi...");
        run_local(args, daemon_rpc_client).await.unwrap();
        println!("Fungi finished");
    }
}

async fn run_local(
    args: FungiArgs,
    daemon_rpc_client: Option<FungiDaemonRpcClient>,
) -> io::Result<()> {
    let mut wasi_rt =
        fungi_wasi::WasiRuntime::new(args.wasi_root_dir(), args.wasi_bin_dir()).unwrap();

    loop {
        let mut stdout = std::io::stdout();
        stdout.write_all(b" # ")?;
        stdout.flush()?;

        // read line and parse
        let stdin = io::stdin();
        let mut reader = BufReader::new(stdin);
        let mut line = String::new();
        let bytes_read = reader.read_line(&mut line).await?;

        if bytes_read == 0 {
            eprintln!("EOF");
            break;
        }

        let child_args: Vec<String> = line.split_whitespace().map(|s| s.to_string()).collect();
        if child_args.is_empty() {
            continue;
        }

        // run cmd
        if let Err(e) = wasi_rt
            .run(child_args, None, daemon_rpc_client.clone(), args.ipc_dir())
            .await
        {
            eprintln!("{:?}", e);
        }
    }
    Ok(())
}

async fn run_remote(mut daemon_ipc_stream: Stream, remote_peer: PeerId) {
    let msg = bincode::serialize(&DaemonMessage::RemoteRequest(remote_peer)).unwrap();
    daemon_ipc_stream.write_all(&msg).await.unwrap();
    let mut buf = [0; 1024];
    let n = daemon_ipc_stream.read(&mut buf).await.unwrap();
    let response = bincode::deserialize::<DaemonMessage>(&buf[..n]).unwrap();
    match response {
        DaemonMessage::RemoteResponse(Ok(ipc_server_name)) => {
            connect_to_forward_task(ipc_server_name).await;
        }
        DaemonMessage::RemoteResponse(Err(e)) => {
            println!("Failed to connect to WASI: {}", e);
        }
        _ => {
            println!("Unexpected response");
        }
    }
}

/// connect to the forward task which provided by the fungi-daemon
/// [main] request connecting to remote peer ->
/// [daemon] connect to remote peer and create stream forward task and return ipc server name ->
/// [main] connect to ipc server (stream forward task)
async fn connect_to_forward_task(ipc_server_name: String) {
    let (rx, tx) = ipc::connect_ipc(&ipc_server_name).await.unwrap().split();
    println!("Welcome to the Fungi!\n");
    tokio::select! {
        _ = forward_stdin_to_remote(tx) => {}
        _ = fowrard_remote_to_stdout(rx) => {}
        _ = tokio::signal::ctrl_c() => {}
    }
    println!("\nWASI process exited");
    std::process::exit(0);
}

async fn forward_stdin_to_remote(mut remote_tx: SendHalf) -> io::Result<()> {
    let mut stdin = tokio::io::stdin();
    let mut buf = [0; 1024];
    loop {
        let n = stdin.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        let data = bincode::serialize(&ForwardStdioMessage::Stdin(buf[..n].to_vec())).unwrap();
        remote_tx.write_all(&data).await?;
    }
    Ok(())
}

async fn fowrard_remote_to_stdout(mut remote_rx: RecvHalf) -> io::Result<()> {
    let mut stdout = tokio::io::stdout();
    let mut buf = [0; 1024];
    loop {
        let n = remote_rx.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        match bincode::deserialize::<ForwardStdioMessage>(&buf[..n]) {
            Ok(ForwardStdioMessage::Stdout(data)) | Ok(ForwardStdioMessage::Stderr(data)) => {
                stdout.write_all(&data).await?;
                stdout.flush().await?;
            }
            _ => {
                println!("Unexpected message");
            }
        }
    }
    Ok(())
}

fn connect_daemon_rpc(stream: Stream) -> FungiDaemonRpcClient {
    let codec_builder = LengthDelimitedCodec::builder();
    let transport = serde_transport::new(codec_builder.new_framed(stream), Bincode::default());
    FungiDaemonRpcClient::new(Default::default(), transport).spawn()
}
