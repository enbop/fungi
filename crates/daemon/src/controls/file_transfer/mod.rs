mod file_transfer_client;
mod file_transfer_service;
mod ftp_impl;
mod webdav_impl;

pub use file_transfer_client::{
    FileTransferClientsControl, start_ftp_proxy_service, start_webdav_proxy_service,
};
pub use file_transfer_service::FileTransferServiceControl;
use fungi_fs::{DirEntry, Metadata, Result};
use std::path::PathBuf;

#[tarpc::service]
pub trait FileTransferRpc {
    async fn metadata(path: PathBuf) -> Result<Metadata>;

    async fn list(path: PathBuf) -> Result<Vec<DirEntry>>;

    async fn get(path: PathBuf, start_pos: u64) -> Result<Vec<u8>>;

    async fn put(bytes: Vec<u8>, path: PathBuf, start_pos: u64) -> Result<u64>;

    async fn del(path: PathBuf) -> Result<()>;

    async fn rmd(path: PathBuf) -> Result<()>;

    async fn mkd(path: PathBuf) -> Result<()>;

    async fn rename(from: PathBuf, to: PathBuf) -> Result<()>;

    async fn cwd(path: PathBuf) -> Result<()>;

    async fn is_windows() -> bool;
}
