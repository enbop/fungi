use super::{daemon::listeners::MushMessage, FungiArgs};
use fungi_util::ipc;
use fungi_wasi::IpcMessage;
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
    let mut stream = ipc::connect_ipc(&ipc_server_name).await.unwrap();
    let data = bincode::serialize(&IpcMessage::Data("wasi.wasm".to_string())).unwrap();
    stream.write_all(&data).await.unwrap();
}
