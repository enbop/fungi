mod local_listener;
mod server_impl;

use std::path::PathBuf;

use libp2p::Stream;
pub use local_listener::FileTransferLocalListener;
pub use server_impl::FileTransferRpcServer;
use tarpc::{serde_transport, tokio_serde::formats::Bincode};
use tokio_util::{codec::LengthDelimitedCodec, compat::FuturesAsyncReadCompatExt as _};

#[tarpc::service]
pub trait FileTransferRpc {
    async fn metadata(path: PathBuf) -> fungi_fs::Metadata;
}
