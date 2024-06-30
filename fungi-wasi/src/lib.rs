use ipc_channel::{asynch::IpcStream, ipc};
use std::path::PathBuf;
use serde::{Serialize, Deserialize};
use futures::stream::StreamExt;

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum IpcMessage {
    Data(String),
}

pub fn run(fungi_dir: PathBuf) -> String {
    let (server, name) = ipc::IpcOneShotServer::<IpcMessage>::new().unwrap(); 
    tokio::spawn(async move{
        let (client, data) = server.accept().unwrap();
        handle_ipc_client(client.to_stream(), data).await;
    });
    return name;
}

async fn handle_ipc_client(mut stream: IpcStream<IpcMessage>, data: IpcMessage) {
    println!("handle_ipc_client data: {:?}", data);
    loop {
        let msg = stream.next().await;
        println!("handle_ipc_client msg: {:?}", msg);
    }
}