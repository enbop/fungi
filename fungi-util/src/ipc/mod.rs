pub mod messages;

use interprocess::local_socket::{
    self,
    tokio::{prelude::*, Stream},
    ListenerOptions,
};
use std::io;

pub fn create_ipc_listener(ipc_sock_name: &str) -> io::Result<LocalSocketListener> {
    #[cfg(target_os = "windows")]
    let name = ipc_sock_name.to_ns_name::<local_socket::GenericNamespaced>()?;
    #[cfg(not(target_os = "windows"))]
    let name = ipc_sock_name.to_fs_name::<local_socket::GenericFilePath>()?;
    let opts = ListenerOptions::new().name(name);
    opts.create_tokio()
}

pub async fn connect_ipc(ipc_sock_name: &str) -> io::Result<Stream> {
    #[cfg(target_os = "windows")]
    let name = ipc_sock_name.to_ns_name::<local_socket::GenericNamespaced>()?;
    #[cfg(not(target_os = "windows"))]
    let name = ipc_sock_name.to_fs_name::<local_socket::GenericFilePath>()?;
    Stream::connect(name).await
}
