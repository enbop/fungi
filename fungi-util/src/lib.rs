#[cfg(feature = "tcp-tunneling")]
pub mod tcp_tunneling;

pub mod ipc;

use futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::io::{AsyncReadExt as TAsyncReadExt, AsyncWriteExt as TAsyncWriteExt};

pub async fn copy_stream(
    mut stream_fut_trait: impl AsyncRead + AsyncWrite + Unpin,
    mut stream_tokio_trait: impl TAsyncReadExt + TAsyncWriteExt + Unpin,
) {
    let mut buf1 = [0u8; 1024];
    let mut buf2 = [0u8; 1024];
    loop {
        tokio::select! {
            n = stream_fut_trait.read(&mut buf1) => {
                if n.is_err() {
                    break;
                }
                let n = n.unwrap();
                if n == 0 {
                    break;
                }
                if stream_tokio_trait.write_all(&buf1[..n]).await.is_err() {
                    break;
                }
            },
            n = stream_tokio_trait.read(&mut buf2) => {
                if n.is_err() {
                    break;
                }
                let n = n.unwrap();
                if n == 0 {
                    break;
                }
                if stream_fut_trait.write_all(&buf2[..n]).await.is_err() {
                    break;
                }
            },
        }
    }
}
