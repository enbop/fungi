mod wasmtime_wasi_impl;
use tokio::task::JoinHandle;
use wasmtime_wasi_impl::*;

use futures::stream::StreamExt;
use ipc_channel::{
    asynch::IpcStream,
    ipc::{self, IpcOneShotServer},
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum IpcMessage {
    Data(String),
}

pub async fn run(server: IpcOneShotServer<IpcMessage>, root_dir: PathBuf, bin_dir: PathBuf) {
    let mut wasi_runtime = WasiRuntime::new(root_dir, bin_dir);

    let (client, data) = server.accept().unwrap();
    handle_ipc_message(data, &mut wasi_runtime).await;
    
    let mut stream = client.to_stream();
    
    // TODO stream to client  
    loop {
        let Some(Ok(data)) = stream.next().await else {
            // TODO never get break
            break;
        };
        handle_ipc_message(data, &mut wasi_runtime).await;
    }
    println!("Wasi runtime finished");
}

async fn handle_ipc_message(IpcMessage::Data(msg): IpcMessage, wasi: &mut WasiRuntime) {
    let args = msg.split_whitespace().collect::<Vec<&str>>();
    if args.is_empty() {
        return;
    }
    wasi.spawn(args).await;
}
