use std::{
    io,
    path::{Path, PathBuf},
};

use libunftp::{
    auth::DefaultUser,
    storage::{Metadata as _, StorageBackend},
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use unftp_sbe_fs::Filesystem;

#[derive(Debug, Error, Serialize, Deserialize)]
pub enum FileTransferError {
    #[error("Not Found")]
    NotFound,
    #[error("Permission Denied")]
    PermissionDenied,
    #[error("Connection Broken")]
    ConnectionBroken,
    #[error("Other: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, FileTransferError>;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Metadata {
    pub len: u64,
    pub is_dir: bool,
    pub is_file: bool,
    pub is_symlink: bool,
    pub modified: Option<std::time::SystemTime>,
    pub gid: u32,
    pub uid: u32,
    pub links: u64,
    pub permissions: u32, // Using u32 to represent permissions as mode bits
    pub readlink: Option<PathBuf>,
}

impl From<unftp_sbe_fs::Meta> for Metadata {
    fn from(value: unftp_sbe_fs::Meta) -> Self {
        Self {
            len: value.len(),
            is_dir: value.is_dir(),
            is_file: value.is_file(),
            is_symlink: value.is_symlink(),
            modified: value.modified().ok(),
            gid: value.gid(),
            uid: value.uid(),
            links: value.links(),
            permissions: value.permissions().0,
            readlink: value.readlink().map(|p| p.to_path_buf()),
        }
    }
}

impl libunftp::storage::Metadata for Metadata {
    fn len(&self) -> u64 {
        self.len
    }

    fn is_dir(&self) -> bool {
        self.is_dir
    }

    fn is_file(&self) -> bool {
        self.is_file
    }

    fn is_symlink(&self) -> bool {
        self.is_symlink
    }

    fn modified(&self) -> libunftp::storage::Result<std::time::SystemTime> {
        self.modified
            .ok_or(libunftp::storage::ErrorKind::CommandNotImplemented.into())
    }

    fn gid(&self) -> u32 {
        self.gid
    }

    fn uid(&self) -> u32 {
        self.uid
    }

    fn links(&self) -> u64 {
        self.links
    }

    fn permissions(&self) -> libunftp::storage::Permissions {
        libunftp::storage::Permissions(self.permissions)
    }

    fn readlink(&self) -> Option<&Path> {
        self.readlink.as_ref().map(PathBuf::as_path)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FileInfo {
    pub path: PathBuf,
    pub metadata: Metadata,
}

pub struct FileSystemWrapper(Filesystem);

// TODO handle rpc errors
impl FileSystemWrapper {
    pub fn new(path: PathBuf) -> io::Result<Self> {
        if !path.exists() {
            return Err(io::ErrorKind::NotFound.into());
        }
        if !path.is_dir() {
            return Err(io::ErrorKind::NotADirectory.into());
        }
        let inner = Filesystem::new(path)?;
        Ok(Self(inner))
    }

    pub async fn metadata(&self, path: &PathBuf) -> Result<Metadata> {
        self.0
            .metadata(&DefaultUser {}, path)
            .await
            .map_err(|e| FileTransferError::Other(e.to_string())) // TODO handle specific errors
            .map(Metadata::from)
    }

    pub async fn list(&self, path: &PathBuf) -> Result<Vec<FileInfo>> {
        let entries = self
            .0
            .list(&DefaultUser {}, path)
            .await
            .map_err(|e| FileTransferError::Other(e.to_string()))?;

        Ok(entries
            .into_iter()
            .map(|entry| FileInfo {
                path: entry.path,
                metadata: Metadata::from(entry.metadata),
            })
            .collect())
    }

    pub async fn get(&self, path: &PathBuf, start_pos: u64) -> Result<Vec<u8>> {
        let reader = self
            .0
            .get(&DefaultUser {}, path, start_pos)
            .await
            .map_err(|e| FileTransferError::Other(e.to_string()))?;

        let mut buffer = Vec::new();
        tokio::io::copy(&mut Box::pin(reader), &mut buffer)
            .await
            .map_err(|e| FileTransferError::Other(e.to_string()))?;

        Ok(buffer)
    }

    pub async fn put<R>(&self, bytes: R, path: &PathBuf, start_pos: u64) -> Result<u64>
    where
        R: tokio::io::AsyncRead + Send + Sync + 'static + Unpin,
    {
        self.0
            .put(&DefaultUser {}, bytes, path, start_pos)
            .await
            .map_err(|e| FileTransferError::Other(e.to_string()))
    }

    pub async fn del(&self, path: &PathBuf) -> Result<()> {
        self.0
            .del(&DefaultUser {}, path)
            .await
            .map_err(|e| FileTransferError::Other(e.to_string()))
    }

    pub async fn rmd(&self, path: &PathBuf) -> Result<()> {
        self.0
            .rmd(&DefaultUser {}, path)
            .await
            .map_err(|e| FileTransferError::Other(e.to_string()))
    }

    pub async fn mkd(&self, path: &PathBuf) -> Result<()> {
        self.0
            .mkd(&DefaultUser {}, path)
            .await
            .map_err(|e| FileTransferError::Other(e.to_string()))
    }

    pub async fn rename(&self, from: &PathBuf, to: &PathBuf) -> Result<()> {
        self.0
            .rename(&DefaultUser {}, from, to)
            .await
            .map_err(|e| FileTransferError::Other(e.to_string()))
    }

    pub async fn cwd(&self, path: &PathBuf) -> Result<()> {
        self.0
            .cwd(&DefaultUser {}, path)
            .await
            .map_err(|e| FileTransferError::Other(e.to_string()))
    }
}
