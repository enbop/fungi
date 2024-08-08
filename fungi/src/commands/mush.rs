use super::{daemon::listeners::MushMessage, FungiArgs};
use fungi_util::ipc;
use fungi_wasi::IpcMessage;
use interprocess::local_socket::{
    tokio::{RecvHalf, SendHalf},
    traits::tokio::Stream,
};
use std::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub async fn mush(args: &FungiArgs) {
    println!("Connecting to fungi daemon");

    let mut stream = ipc::connect_ipc(&args.mush_ipc_path().to_string_lossy())
        .await
        .unwrap();
    let msg = bincode::serialize(&MushMessage::InitRequest).unwrap();
    stream.write_all(&msg).await.unwrap();
    let mut buf = [0; 1024];
    let n = stream.read(&mut buf).await.unwrap();
    let response = bincode::deserialize::<MushMessage>(&buf[..n]).unwrap();
    match response {
        MushMessage::InitResponse(ipc_server_name) => {
            println!("IPC server name: {}", ipc_server_name);
            connect_to_wasi(ipc_server_name).await;
        }
        _ => {
            println!("Unexpected response");
        }
    }
}

async fn connect_to_wasi(ipc_server_name: String) {
    let (rx, tx) = ipc::connect_ipc(&ipc_server_name).await.unwrap().split();
    println!("Welcome to the Fungi!\n");
    tokio::select! {
        _ = forward_stdin_to_wasi(tx) => {}
        _ = fowrard_wasi_to_stdout(rx) => {}
        _ = tokio::signal::ctrl_c() => {}
    }
    println!("\nWASI process exited");
    std::process::exit(0);
}

async fn forward_stdin_to_wasi(mut wasi_tx: SendHalf) -> io::Result<()> {
    let mut stdin = tokio::io::stdin();
    loop {
        let mut in_buf = [0; 1024];
        let n = stdin.read(&mut in_buf).await?;
        if n == 0 {
            break;
        }
        let data = bincode::serialize(&IpcMessage::Stdin(in_buf[..n].to_vec())).unwrap();
        wasi_tx.write_all(&data).await?;
    }
    Ok(())
}

async fn fowrard_wasi_to_stdout(mut wasi_rx: RecvHalf) -> io::Result<()> {
    let mut stdout = tokio::io::stdout();
    let mut buf = [0; 1024];
    loop {
        let n = wasi_rx.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        match bincode::deserialize::<IpcMessage>(&buf[..n]) {
            Ok(IpcMessage::Stdout(data)) | Ok(IpcMessage::Stderr(data)) => {
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
