use std::{io, path::PathBuf};

use libunftp::{
    auth::DefaultUser,
    storage::{Metadata as _, StorageBackend},
};
use serde::{Deserialize, Serialize};
use unftp_sbe_fs::Filesystem;

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

pub struct FileSystemWrapper(Filesystem);

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

    pub async fn metadata(&self, path: &PathBuf) -> Metadata {
        // TODO handle errors properly
        self.0.metadata(&DefaultUser {}, path).await.unwrap().into()
    }
}
