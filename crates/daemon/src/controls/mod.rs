mod file_transfer_service;

pub use file_transfer_service::FileTransferServiceControl;

use std::path::PathBuf;

#[tarpc::service]
pub trait FileTransferRpc {
    async fn metadata(path: PathBuf) -> fungi_fs::Metadata;
}
