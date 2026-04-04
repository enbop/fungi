use std::time::SystemTime;

use async_trait::async_trait;
use typed_path::{Utf8Component, Utf8Path, Utf8PathBuf, Utf8UnixEncoding};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendError {
    NotFound { path: String },
    PermissionDenied { path: String },
    AlreadyExists { path: String },
    DirectoryNotEmpty { path: String },
    IsDirectory { path: String },
    NotDirectory { path: String },
    InvalidPath { path: String },
    NoSpace,
    FileTooLarge,
    ReadOnly,
    NotSupported { operation: String },
    Other { message: String },
}

pub type Result<T> = std::result::Result<T, BackendError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metadata {
    pub len: u64,
    pub is_dir: bool,
    pub is_file: bool,
    pub is_symlink: bool,
    pub modified: Option<SystemTime>,
    pub created: Option<SystemTime>,
    pub accessed: Option<SystemTime>,
    pub permissions: u32,
}

impl Metadata {
    pub fn directory() -> Self {
        let now = SystemTime::now();
        Self {
            len: 0,
            is_dir: true,
            is_file: false,
            is_symlink: false,
            modified: Some(now),
            created: Some(now),
            accessed: Some(now),
            permissions: 0o755,
        }
    }

    pub fn file(len: u64) -> Self {
        let now = SystemTime::now();
        Self {
            len,
            is_dir: false,
            is_file: true,
            is_symlink: false,
            modified: Some(now),
            created: Some(now),
            accessed: Some(now),
            permissions: 0o644,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub name: String,
    pub metadata: Metadata,
}

#[async_trait]
pub trait WebDavBackend: Clone + Send + Sync + 'static {
    async fn metadata(&self, path: &str) -> Result<Metadata>;
    async fn read_dir(&self, path: &str) -> Result<Vec<DirEntry>>;
    async fn read_chunk(&self, path: &str, start: u64, length: u64) -> Result<Vec<u8>>;
    async fn write_chunk(&self, path: &str, start: u64, bytes: Vec<u8>) -> Result<u64>;
    async fn create_dir(&self, path: &str) -> Result<()>;
    async fn remove_dir(&self, path: &str) -> Result<()>;
    async fn remove_file(&self, path: &str) -> Result<()>;
    async fn rename(&self, from: &str, to: &str) -> Result<()>;
    async fn copy(&self, from: &str, to: &str) -> Result<()>;
}

pub fn normalize_path(path: &str) -> Result<String> {
    let unix_path = convert_string_to_utf8_unix_path_buf(path);
    if unix_path
        .components()
        .any(|component| component.is_parent())
    {
        return Err(BackendError::InvalidPath {
            path: path.to_string(),
        });
    }

    let normalized = unix_path.normalize();
    let trimmed = normalized.as_str().trim_matches('/');
    if trimmed.is_empty() || trimmed == "." {
        Ok(String::new())
    } else {
        Ok(trimmed.to_string())
    }
}

fn convert_string_to_utf8_unix_path_buf(path: &str) -> Utf8PathBuf<Utf8UnixEncoding> {
    #[cfg(windows)]
    {
        let windows_path = Utf8Path::<typed_path::Utf8WindowsEncoding>::new(path);
        windows_path.with_encoding::<Utf8UnixEncoding>()
    }
    #[cfg(not(windows))]
    {
        Utf8Path::<Utf8UnixEncoding>::new(path).with_encoding()
    }
}
