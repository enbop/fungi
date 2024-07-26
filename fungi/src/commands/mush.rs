use fungi_wasi::IpcMessage;
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::TcpStream};
use super::daemon::listeners::MushMessage;
use ipc_channel::ipc;

pub async fn mush() {
    println!("Connecting to fungi daemon");
    let mut stream = TcpStream::connect(format!("127.0.0.1:6010")).await.unwrap();
    let msg = bincode::serialize(&MushMessage::InitRequest).unwrap();
    stream.write_all(&msg).await.unwrap();
    let mut buf = [0; 1024];
    let n = stream.read(&mut buf).await.unwrap();
    let response = bincode::deserialize::<MushMessage>(&buf[..n]).unwrap();
    match response {
        MushMessage::InitResponse(ipc_server_name) => {
            println!("IPC server name: {}", ipc_server_name);
            connect_to_wasi(ipc_server_name);
        }
        _ => {
            println!("Unexpected response");
        }
    }
}

fn connect_to_wasi(ipc_server_name: String) {
    let wasi: ipc::IpcSender<IpcMessage> = ipc::IpcSender::connect(ipc_server_name).unwrap();
    wasi.send(IpcMessage::Data("wasi.wasm".to_string())).unwrap();
}