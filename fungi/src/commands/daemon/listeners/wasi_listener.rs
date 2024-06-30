use std::{collections::HashMap, process::Stdio, sync::Arc};

use futures::lock::Mutex;
use tokio::{
    io::AsyncReadExt,
    process::{Child, Command},
};

#[derive(Clone)]
pub struct WasiListener {
    child_process_map: Arc<Mutex<HashMap<String, Child>>>, // TODO
}

impl WasiListener {
    pub fn new() -> Self {
        Self {
            child_process_map: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn spawn_wasi_process(&self) -> String {
        let self_bin = std::env::current_exe().unwrap();
        let mut child = Command::new(self_bin)
            .arg("wasi")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();

        let mut buf = [0; 1024];
        let n = child.stdout.as_mut().unwrap().read(&mut buf).await.unwrap();
        let msg = String::from_utf8_lossy(&buf[..n]);
        tokio::spawn(async move {
            loop {
                let mut buf = [0; 1024];
                let n = child.stdout.as_mut().unwrap().read(&mut buf).await.unwrap();
                let msg = String::from_utf8_lossy(&buf[..n]);
                println!("child msg: {}", msg);
            }
        });

        msg.trim().to_string()
    }
}
