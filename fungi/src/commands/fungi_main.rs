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

    println!("Starting Fungi...");
    run_local(args).await.unwrap();
    println!("Fungi finished");
}

async fn run_local(args: FungiArgs) -> io::Result<()> {
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
    }
    Ok(())
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
