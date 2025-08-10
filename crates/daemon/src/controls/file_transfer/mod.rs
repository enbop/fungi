mod file_transfer_client;
mod file_transfer_service;
mod ftp_impl;
mod webdav_impl;

pub use file_transfer_client::{
    FileTransferClientsControl, start_ftp_proxy_service, start_webdav_proxy_service,
};
pub use file_transfer_service::FileTransferServiceControl;
use fungi_fs::{DirEntry, Metadata, Result};

#[tarpc::service]
pub trait FileTransferRpc {
    async fn metadata(unix_path: String) -> Result<Metadata>;

    async fn list(unix_path: String) -> Result<Vec<DirEntry>>;

    async fn get(unix_path: String, start_pos: u64) -> Result<Vec<u8>>;

    async fn put(bytes: Vec<u8>, unix_path: String, start_pos: u64) -> Result<u64>;

    async fn del(unix_path: String) -> Result<()>;

    async fn rmd(unix_path: String) -> Result<()>;

    async fn mkd(unix_path: String) -> Result<()>;

    async fn rename(from: String, to: String) -> Result<()>;

    async fn cwd(unix_path: String) -> Result<()>;

    async fn is_windows() -> bool;
}
