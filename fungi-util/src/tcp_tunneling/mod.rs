mod port_forward;
pub use port_forward::*;
mod port_listen;
pub use port_listen::*;

use futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::io::{AsyncReadExt as TAsyncReadExt, AsyncWriteExt as TAsyncWriteExt};
use tokio::net::TcpStream;

async fn copy_stream(
    mut p2p_sub_stream: impl AsyncRead + AsyncWrite + Unpin,
    mut local_tcp_stream: TcpStream,
) {
    let mut buf1 = [0u8; 1024];
    let mut buf2 = [0u8; 1024];
    loop {
        tokio::select! {
            n = p2p_sub_stream.read(&mut buf1) => {
                if n.is_err() {
                    break;
                }
                let n = n.unwrap();
                if n == 0 {
                    break;
                }
                if local_tcp_stream.write_all(&buf1[..n]).await.is_err() {
                    break;
                }
            },
            n = local_tcp_stream.read(&mut buf2) => {
                if n.is_err() {
                    break;
                }
                let n = n.unwrap();
                if n == 0 {
                    break;
                }
                if p2p_sub_stream.write_all(&buf2[..n]).await.is_err() {
                    break;
                }
            },
        }
    }
}
