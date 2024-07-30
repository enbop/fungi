use super::daemon::listeners::MushMessage;
use fungi_wasi::IpcMessage;
use interprocess::local_socket::{
    tokio::{prelude::*, Stream},
    GenericNamespaced, ToNsName,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

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
            connect_to_wasi(ipc_server_name).await;
        }
        _ => {
            println!("Unexpected response");
        }
    }
}

async fn connect_to_wasi(ipc_server_name: String) {
    let name = ipc_server_name.to_ns_name::<GenericNamespaced>().unwrap();
    let mut stream = Stream::connect(name).await.unwrap();
    let data = bincode::serialize(&IpcMessage::Data("wasi.wasm".to_string())).unwrap();
    stream.write_all(&data).await.unwrap();
}
