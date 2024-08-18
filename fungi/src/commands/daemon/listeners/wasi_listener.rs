use libp2p::PeerId;
use std::{
    collections::HashMap,
    io,
    process::Stdio,
    sync::{Arc, Mutex},
};
use tokio::{
    io::AsyncReadExt,
    process::{Child, Command},
};

use crate::commands::daemon::ALL_IN_ONE_BINARY;

struct WasiChild {
    process: Child,
    ipc_name: String,
    remote_peer_id: Option<PeerId>,
}

#[derive(Clone)]
pub struct WasiListener {
    child_process_map: Arc<Mutex<HashMap<String, WasiChild>>>, // TODO
}

impl WasiListener {
    pub fn new() -> Self {
        Self {
            child_process_map: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn spawn_wasi_process(
        &mut self,
        remote_peer_id: Option<PeerId>,
    ) -> io::Result<String> {
        let self_bin = std::env::current_exe()?;
        let mut child = if *ALL_IN_ONE_BINARY.get().unwrap() {
            Command::new(self_bin)
                .arg("wasi")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()?
        } else {
            let wasi_path = self_bin.parent().unwrap().join("fungi-wasi");
            Command::new(wasi_path)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()?
        };

        let mut buf = [0; 1024];

        let mut child_stdout = child.stdout.take().unwrap();
        let n = child_stdout.read(&mut buf).await?;
        let msg = String::from_utf8_lossy(&buf[..n]);
        let name = msg.trim().to_string();
        tokio::spawn(async move {
            loop {
                let mut buf = [0; 1024];
                let n = child_stdout.read(&mut buf).await.unwrap();
                if n == 0 {
                    break;
                }
                let msg = String::from_utf8_lossy(&buf[..n]);
                println!("child msg: {}", msg);
            }
            println!("child process exited");
        });
        self.child_process_map.lock().unwrap().insert(
            name.clone(),
            WasiChild {
                process: child,
                ipc_name: name.clone(),
                remote_peer_id,
            },
        );
        Ok(name)
    }
}
