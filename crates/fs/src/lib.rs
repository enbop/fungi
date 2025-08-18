use std::{
    io::{self, SeekFrom},
    path::{Path, PathBuf},
    pin::Pin,
    task::{Context, Poll},
    time::SystemTime,
};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{
    fs::{File, OpenOptions as TokioOpenOptions},
    io::{AsyncRead, AsyncReadExt, AsyncSeek, AsyncSeekExt, AsyncWrite, ReadBuf},
};

#[derive(Debug, Error, Serialize, Deserialize, Clone, PartialEq)]
pub enum FileSystemError {
    #[error("File or directory not found: {path}")]
    NotFound { path: String },

    #[error("Permission denied: {path}")]
    PermissionDenied { path: String },

    #[error("File or directory already exists: {path}")]
    AlreadyExists { path: String },

    #[error("Directory not empty: {path}")]
    DirectoryNotEmpty { path: String },

    #[error("Is a directory: {path}")]
    IsDirectory { path: String },

    #[error("Not a directory: {path}")]
    NotDirectory { path: String },

    #[error("Invalid path: {path}")]
    InvalidPath { path: String },

    #[error("No space left on device")]
    NoSpace,

    #[error("File too large")]
    FileTooLarge,

    #[error("Read-only filesystem")]
    ReadOnly,

    #[error("Connection broken")]
    ConnectionBroken,

    #[error("Operation not supported: {operation}")]
    NotSupported { operation: String },

    #[error("IO error: {message}")]
    Io { message: String },

    #[error("Other error: {message}")]
    Other { message: String },
}

impl From<io::Error> for FileSystemError {
    fn from(err: io::Error) -> Self {
        match err.kind() {
            io::ErrorKind::NotFound => FileSystemError::NotFound {
                path: "unknown".to_string(),
            },
            io::ErrorKind::PermissionDenied => FileSystemError::PermissionDenied {
                path: "unknown".to_string(),
            },
            io::ErrorKind::AlreadyExists => FileSystemError::AlreadyExists {
                path: "unknown".to_string(),
            },
            io::ErrorKind::InvalidInput | io::ErrorKind::InvalidData => {
                FileSystemError::InvalidPath {
                    path: "unknown".to_string(),
                }
            }
            _ => FileSystemError::Io {
                message: err.to_string(),
            },
        }
    }
}

impl From<FileSystemError> for io::Error {
    fn from(err: FileSystemError) -> Self {
        match err {
            FileSystemError::NotFound { .. } => {
                io::Error::new(io::ErrorKind::NotFound, err.to_string())
            }
            FileSystemError::PermissionDenied { .. } => {
                io::Error::new(io::ErrorKind::PermissionDenied, err.to_string())
            }
            FileSystemError::AlreadyExists { .. } => {
                io::Error::new(io::ErrorKind::AlreadyExists, err.to_string())
            }
            FileSystemError::InvalidPath { .. } => {
                io::Error::new(io::ErrorKind::InvalidInput, err.to_string())
            }
            FileSystemError::NoSpace => io::Error::new(io::ErrorKind::OutOfMemory, err.to_string()),
            FileSystemError::ReadOnly => {
                io::Error::new(io::ErrorKind::PermissionDenied, err.to_string())
            }
            FileSystemError::ConnectionBroken => {
                io::Error::new(io::ErrorKind::ConnectionAborted, err.to_string())
            }
            _ => io::Error::other(err.to_string()),
        }
    }
}

pub type Result<T> = std::result::Result<T, FileSystemError>;

/// File metadata with comprehensive information
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Metadata {
    pub len: u64,
    pub is_dir: bool,
    pub is_file: bool,
    pub is_symlink: bool,
    pub modified: Option<SystemTime>,
    pub created: Option<SystemTime>,
    pub accessed: Option<SystemTime>,
    pub permissions: u32,
    pub uid: u32,
    pub gid: u32,
    pub links: u64,
    pub readlink: Option<PathBuf>,
}

impl From<std::fs::Metadata> for Metadata {
    fn from(value: std::fs::Metadata) -> Self {
        Self {
            len: value.len(),
            is_dir: value.is_dir(),
            is_file: value.is_file(),
            is_symlink: value.file_type().is_symlink(),
            modified: value.modified().ok(),
            created: value.created().ok(),
            accessed: value.accessed().ok(),
            #[cfg(unix)]
            permissions: {
                use std::os::unix::fs::MetadataExt;
                value.mode()
            },
            #[cfg(not(unix))]
            permissions: if value.permissions().readonly() {
                0o444
            } else {
                0o666
            },
            #[cfg(unix)]
            uid: {
                use std::os::unix::fs::MetadataExt;
                value.uid()
            },
            #[cfg(not(unix))]
            uid: 0,
            #[cfg(unix)]
            gid: {
                use std::os::unix::fs::MetadataExt;
                value.gid()
            },
            #[cfg(not(unix))]
            gid: 0,
            #[cfg(unix)]
            links: {
                use std::os::unix::fs::MetadataExt;
                value.nlink()
            },
            #[cfg(not(unix))]
            links: 1,
            readlink: None, // Will be set separately for symlinks
        }
    }
}

/// Directory entry information
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DirEntry {
    pub name: String,
    pub path: PathBuf,
    pub metadata: Metadata,
}

/// File open options for creating/opening files
#[derive(Debug, Clone, Default)]
pub struct OpenOptions {
    pub read: bool,
    pub write: bool,
    pub append: bool,
    pub truncate: bool,
    pub create: bool,
    pub create_new: bool,
}

impl OpenOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn read(mut self, read: bool) -> Self {
        self.read = read;
        self
    }

    pub fn write(mut self, write: bool) -> Self {
        self.write = write;
        self
    }

    pub fn append(mut self, append: bool) -> Self {
        self.append = append;
        self
    }

    pub fn truncate(mut self, truncate: bool) -> Self {
        self.truncate = truncate;
        self
    }

    pub fn create(mut self, create: bool) -> Self {
        self.create = create;
        self
    }

    pub fn create_new(mut self, create_new: bool) -> Self {
        self.create_new = create_new;
        self
    }

    /// Convert to tokio OpenOptions
    fn to_tokio_options(&self) -> TokioOpenOptions {
        let mut options = TokioOpenOptions::new();
        options
            .read(self.read)
            .write(self.write)
            .append(self.append)
            .truncate(self.truncate)
            .create(self.create)
            .create_new(self.create_new);
        options
    }
}

/// A streaming file handle that implements AsyncRead, AsyncWrite, and AsyncSeek
pub struct FileStream {
    file: File,
    path: PathBuf,
}

impl FileStream {
    async fn new(path: PathBuf, options: &OpenOptions) -> Result<Self> {
        let file = options
            .to_tokio_options()
            .open(&path)
            .await
            .map_err(|e| match e.kind() {
                io::ErrorKind::NotFound => FileSystemError::NotFound {
                    path: path.to_string_lossy().to_string(),
                },
                io::ErrorKind::PermissionDenied => FileSystemError::PermissionDenied {
                    path: path.to_string_lossy().to_string(),
                },
                io::ErrorKind::AlreadyExists => FileSystemError::AlreadyExists {
                    path: path.to_string_lossy().to_string(),
                },
                _ => FileSystemError::Io {
                    message: format!("Failed to open {}: {}", path.display(), e),
                },
            })?;

        Ok(Self { file, path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub async fn metadata(&self) -> Result<Metadata> {
        let meta = self
            .file
            .metadata()
            .await
            .map_err(|e| FileSystemError::Io {
                message: format!("Failed to get metadata for {}: {}", self.path.display(), e),
            })?;
        Ok(Metadata::from(meta))
    }

    pub async fn sync_all(&self) -> Result<()> {
        self.file.sync_all().await.map_err(|e| FileSystemError::Io {
            message: format!("Failed to sync {}: {}", self.path.display(), e),
        })
    }

    pub async fn sync_data(&self) -> Result<()> {
        self.file
            .sync_data()
            .await
            .map_err(|e| FileSystemError::Io {
                message: format!("Failed to sync data for {}: {}", self.path.display(), e),
            })
    }
}

impl AsyncRead for FileStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.file).poll_read(cx, buf)
    }
}

impl AsyncWrite for FileStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.file).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.file).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.file).poll_shutdown(cx)
    }
}

impl AsyncSeek for FileStream {
    fn start_seek(mut self: Pin<&mut Self>, position: SeekFrom) -> io::Result<()> {
        Pin::new(&mut self.file).start_seek(position)
    }

    fn poll_complete(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
        Pin::new(&mut self.file).poll_complete(cx)
    }
}

/// Core filesystem implementation
pub struct FileSystem {
    root: PathBuf,
}

impl FileSystem {
    /// Create a new filesystem rooted at the given path
    pub fn new<P: AsRef<Path>>(root: P) -> Result<Self> {
        let root = root.as_ref().to_path_buf();

        if !root.exists() {
            return Err(FileSystemError::NotFound {
                path: root.to_string_lossy().to_string(),
            });
        }

        if !root.is_dir() {
            return Err(FileSystemError::NotDirectory {
                path: root.to_string_lossy().to_string(),
            });
        }

        Ok(Self { root })
    }

    /// Get the root directory
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Resolve a relative path to an absolute path within the filesystem
    fn resolve_path<P: AsRef<Path>>(&self, path: P) -> Result<PathBuf> {
        let path = path.as_ref();

        // Convert to absolute path within our root
        let mut absolute_path = self.root.clone();

        // Handle different path formats
        let clean_path = if path.is_absolute() {
            // Strip the leading slash and treat as relative to root
            path.strip_prefix("/").unwrap_or(path)
        } else {
            path
        };

        // Remove . and .. components and ensure we stay within root
        for component in clean_path.components() {
            match component {
                std::path::Component::Normal(name) => {
                    absolute_path.push(name);
                }
                std::path::Component::ParentDir => {
                    if !absolute_path.pop() || absolute_path < self.root {
                        return Err(FileSystemError::InvalidPath {
                            path: path.to_string_lossy().to_string(),
                        });
                    }
                }
                std::path::Component::CurDir => {
                    // Do nothing for current dir
                }
                _ => {
                    return Err(FileSystemError::InvalidPath {
                        path: path.to_string_lossy().to_string(),
                    });
                }
            }
        }

        // Ensure the resolved path is still within our root
        if !absolute_path.starts_with(&self.root) {
            return Err(FileSystemError::InvalidPath {
                path: path.to_string_lossy().to_string(),
            });
        }

        Ok(absolute_path)
    }

    /// Get metadata for a file or directory
    pub async fn metadata<P: AsRef<Path>>(&self, path: P) -> Result<Metadata> {
        let full_path = self.resolve_path(&path)?;

        let meta = tokio::fs::metadata(&full_path)
            .await
            .map_err(|e| match e.kind() {
                io::ErrorKind::NotFound => FileSystemError::NotFound {
                    path: path.as_ref().to_string_lossy().to_string(),
                },
                io::ErrorKind::PermissionDenied => FileSystemError::PermissionDenied {
                    path: path.as_ref().to_string_lossy().to_string(),
                },
                _ => FileSystemError::Io {
                    message: format!(
                        "Failed to get metadata for {}: {}",
                        path.as_ref().display(),
                        e
                    ),
                },
            })?;

        let mut metadata = Metadata::from(meta);

        // Handle symlinks
        if metadata.is_symlink {
            if let Ok(target) = tokio::fs::read_link(&full_path).await {
                metadata.readlink = Some(target);
            }
        }

        Ok(metadata)
    }

    /// List directory contents
    pub async fn list_dir<P: AsRef<Path>>(&self, path: P) -> Result<Vec<DirEntry>> {
        let full_path = self.resolve_path(&path)?;

        let mut entries = Vec::new();
        let mut dir = tokio::fs::read_dir(&full_path)
            .await
            .map_err(|e| match e.kind() {
                io::ErrorKind::NotFound => FileSystemError::NotFound {
                    path: path.as_ref().to_string_lossy().to_string(),
                },
                io::ErrorKind::PermissionDenied => FileSystemError::PermissionDenied {
                    path: path.as_ref().to_string_lossy().to_string(),
                },
                io::ErrorKind::NotADirectory => FileSystemError::NotDirectory {
                    path: path.as_ref().to_string_lossy().to_string(),
                },
                _ => FileSystemError::Io {
                    message: format!(
                        "Failed to read directory {}: {}",
                        path.as_ref().display(),
                        e
                    ),
                },
            })?;

        while let Some(entry) = dir.next_entry().await.map_err(|e| FileSystemError::Io {
            message: format!("Failed to read directory entry: {e}"),
        })? {
            let name = entry.file_name().to_string_lossy().to_string();
            let entry_path = entry.path();

            // Get relative path from root
            let relative_path = entry_path
                .strip_prefix(&self.root)
                .unwrap_or(&entry_path)
                .to_path_buf();

            let metadata = match entry.metadata().await {
                Ok(meta) => {
                    let mut metadata = Metadata::from(meta);
                    if metadata.is_symlink {
                        if let Ok(target) = tokio::fs::read_link(&entry_path).await {
                            metadata.readlink = Some(target);
                        }
                    }
                    metadata
                }
                Err(_) => {
                    // Skip entries we can't read metadata for
                    continue;
                }
            };

            entries.push(DirEntry {
                name,
                path: relative_path,
                metadata,
            });
        }

        // Sort entries by name for consistent ordering
        entries.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(entries)
    }

    /// Open a file for reading/writing
    pub async fn open<P: AsRef<Path>>(&self, path: P, options: &OpenOptions) -> Result<FileStream> {
        let full_path = self.resolve_path(&path)?;

        // Create parent directories if needed and we're creating the file
        if options.create || options.create_new {
            if let Some(parent) = full_path.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|e| FileSystemError::Io {
                        message: format!(
                            "Failed to create parent directories for {}: {}",
                            path.as_ref().display(),
                            e
                        ),
                    })?;
            }
        }

        FileStream::new(full_path, options).await
    }

    /// Read entire file contents
    pub async fn read_to_vec<P: AsRef<Path>>(&self, path: P) -> Result<Vec<u8>> {
        let full_path = self.resolve_path(&path)?;

        tokio::fs::read(&full_path)
            .await
            .map_err(|e| match e.kind() {
                io::ErrorKind::NotFound => FileSystemError::NotFound {
                    path: path.as_ref().to_string_lossy().to_string(),
                },
                io::ErrorKind::PermissionDenied => FileSystemError::PermissionDenied {
                    path: path.as_ref().to_string_lossy().to_string(),
                },
                io::ErrorKind::IsADirectory => FileSystemError::IsDirectory {
                    path: path.as_ref().to_string_lossy().to_string(),
                },
                _ => FileSystemError::Io {
                    message: format!("Failed to read {}: {}", path.as_ref().display(), e),
                },
            })
    }

    /// Read file contents starting from a specific position
    pub async fn read_from_position<P: AsRef<Path>>(
        &self,
        path: P,
        start_pos: u64,
    ) -> Result<Vec<u8>> {
        if start_pos == 0 {
            // Simple case: read entire file
            self.read_to_vec(path).await
        } else {
            // Need to seek and read
            let options = OpenOptions::new().read(true);
            let mut file = self.open(path, &options).await?;

            file.seek(SeekFrom::Start(start_pos))
                .await
                .map_err(|e| FileSystemError::Io {
                    message: format!("Failed to seek: {e}"),
                })?;

            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer)
                .await
                .map_err(|e| FileSystemError::Io {
                    message: format!("Failed to read: {e}"),
                })?;

            Ok(buffer)
        }
    }

    /// Read a specific chunk of bytes from a file starting at a position
    pub async fn read_chunk<P: AsRef<Path>>(
        &self,
        path: P,
        start_pos: u64,
        length: u64,
    ) -> Result<Vec<u8>> {
        let options = OpenOptions::new().read(true);
        let mut file = self.open(path, &options).await?;

        file.seek(SeekFrom::Start(start_pos))
            .await
            .map_err(|e| FileSystemError::Io {
                message: format!("Failed to seek: {e}"),
            })?;

        let mut buffer = vec![0u8; length as usize];
        let bytes_read = file.read(&mut buffer)
            .await
            .map_err(|e| FileSystemError::Io {
                message: format!("Failed to read: {e}"),
            })?;

        // Resize buffer to actual bytes read
        buffer.truncate(bytes_read);
        Ok(buffer)
    }

    /// Write data to file at a specific position
    pub async fn write_at_position<P: AsRef<Path>, R>(
        &self,
        path: P,
        mut data: R,
        start_pos: u64,
    ) -> Result<u64>
    where
        R: AsyncRead + Unpin,
    {
        if start_pos == 0 {
            // Simple case: overwrite entire file
            let mut buffer = Vec::new();
            tokio::io::copy(&mut data, &mut buffer)
                .await
                .map_err(|e| FileSystemError::Io {
                    message: format!("Failed to read data: {e}"),
                })?;

            self.write(&path, &buffer).await?;
            Ok(buffer.len() as u64)
        } else {
            // Need to seek and write
            let options = OpenOptions::new().write(true).create(true);
            let mut file = self.open(&path, &options).await?;

            file.seek(SeekFrom::Start(start_pos))
                .await
                .map_err(|e| FileSystemError::Io {
                    message: format!("Failed to seek: {e}"),
                })?;

            let bytes_written =
                tokio::io::copy(&mut data, &mut file)
                    .await
                    .map_err(|e| FileSystemError::Io {
                        message: format!("Failed to write: {e}"),
                    })?;

            file.sync_all().await?;
            Ok(bytes_written)
        }
    }

    /// Write bytes to file at a specific position
    pub async fn write_bytes_at_position<P: AsRef<Path>>(
        &self,
        path: P,
        data: Vec<u8>,
        start_pos: u64,
    ) -> Result<u64> {
        let cursor = std::io::Cursor::new(data);
        self.write_at_position(path, cursor, start_pos).await
    }

    /// Write data to file
    pub async fn write<P: AsRef<Path>>(&self, path: P, data: &[u8]) -> Result<()> {
        let full_path = self.resolve_path(&path)?;

        // Create parent directories if needed
        if let Some(parent) = full_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| FileSystemError::Io {
                    message: format!(
                        "Failed to create parent directories for {}: {}",
                        path.as_ref().display(),
                        e
                    ),
                })?;
        }

        tokio::fs::write(&full_path, data)
            .await
            .map_err(|e| match e.kind() {
                io::ErrorKind::PermissionDenied => FileSystemError::PermissionDenied {
                    path: path.as_ref().to_string_lossy().to_string(),
                },
                io::ErrorKind::IsADirectory => FileSystemError::IsDirectory {
                    path: path.as_ref().to_string_lossy().to_string(),
                },
                _ => FileSystemError::Io {
                    message: format!("Failed to write to {}: {}", path.as_ref().display(), e),
                },
            })
    }

    /// Create a directory
    pub async fn create_dir<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let full_path = self.resolve_path(&path)?;

        tokio::fs::create_dir(&full_path)
            .await
            .map_err(|e| match e.kind() {
                io::ErrorKind::AlreadyExists => FileSystemError::AlreadyExists {
                    path: path.as_ref().to_string_lossy().to_string(),
                },
                io::ErrorKind::PermissionDenied => FileSystemError::PermissionDenied {
                    path: path.as_ref().to_string_lossy().to_string(),
                },
                _ => FileSystemError::Io {
                    message: format!(
                        "Failed to create directory {}: {}",
                        path.as_ref().display(),
                        e
                    ),
                },
            })
    }

    /// Create directories recursively
    pub async fn create_dir_all<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let full_path = self.resolve_path(&path)?;

        tokio::fs::create_dir_all(&full_path)
            .await
            .map_err(|e| match e.kind() {
                io::ErrorKind::PermissionDenied => FileSystemError::PermissionDenied {
                    path: path.as_ref().to_string_lossy().to_string(),
                },
                _ => FileSystemError::Io {
                    message: format!(
                        "Failed to create directories {}: {}",
                        path.as_ref().display(),
                        e
                    ),
                },
            })
    }

    /// Remove a file
    pub async fn remove_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let full_path = self.resolve_path(&path)?;

        tokio::fs::remove_file(&full_path)
            .await
            .map_err(|e| match e.kind() {
                io::ErrorKind::NotFound => FileSystemError::NotFound {
                    path: path.as_ref().to_string_lossy().to_string(),
                },
                io::ErrorKind::PermissionDenied => FileSystemError::PermissionDenied {
                    path: path.as_ref().to_string_lossy().to_string(),
                },
                io::ErrorKind::IsADirectory => FileSystemError::IsDirectory {
                    path: path.as_ref().to_string_lossy().to_string(),
                },
                _ => FileSystemError::Io {
                    message: format!("Failed to remove file {}: {}", path.as_ref().display(), e),
                },
            })
    }

    /// Remove an empty directory
    pub async fn remove_dir<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let full_path = self.resolve_path(&path)?;

        tokio::fs::remove_dir(&full_path)
            .await
            .map_err(|e| match e.kind() {
                io::ErrorKind::NotFound => FileSystemError::NotFound {
                    path: path.as_ref().to_string_lossy().to_string(),
                },
                io::ErrorKind::PermissionDenied => FileSystemError::PermissionDenied {
                    path: path.as_ref().to_string_lossy().to_string(),
                },
                io::ErrorKind::DirectoryNotEmpty => FileSystemError::DirectoryNotEmpty {
                    path: path.as_ref().to_string_lossy().to_string(),
                },
                _ => FileSystemError::Io {
                    message: format!(
                        "Failed to remove directory {}: {}",
                        path.as_ref().display(),
                        e
                    ),
                },
            })
    }

    /// Remove a directory and all its contents recursively
    pub async fn remove_dir_all<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let full_path = self.resolve_path(&path)?;

        tokio::fs::remove_dir_all(&full_path)
            .await
            .map_err(|e| match e.kind() {
                io::ErrorKind::NotFound => FileSystemError::NotFound {
                    path: path.as_ref().to_string_lossy().to_string(),
                },
                io::ErrorKind::PermissionDenied => FileSystemError::PermissionDenied {
                    path: path.as_ref().to_string_lossy().to_string(),
                },
                _ => FileSystemError::Io {
                    message: format!(
                        "Failed to remove directory tree {}: {}",
                        path.as_ref().display(),
                        e
                    ),
                },
            })
    }

    /// Rename/move a file or directory
    pub async fn rename<P: AsRef<Path>, Q: AsRef<Path>>(&self, from: P, to: Q) -> Result<()> {
        let from_path = self.resolve_path(&from)?;
        let to_path = self.resolve_path(&to)?;

        // Create parent directory of destination if needed
        if let Some(parent) = to_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| FileSystemError::Io {
                    message: format!(
                        "Failed to create parent directories for {}: {}",
                        to.as_ref().display(),
                        e
                    ),
                })?;
        }

        tokio::fs::rename(&from_path, &to_path)
            .await
            .map_err(|e| match e.kind() {
                io::ErrorKind::NotFound => FileSystemError::NotFound {
                    path: from.as_ref().to_string_lossy().to_string(),
                },
                io::ErrorKind::PermissionDenied => FileSystemError::PermissionDenied {
                    path: from.as_ref().to_string_lossy().to_string(),
                },
                io::ErrorKind::AlreadyExists => FileSystemError::AlreadyExists {
                    path: to.as_ref().to_string_lossy().to_string(),
                },
                _ => FileSystemError::Io {
                    message: format!(
                        "Failed to rename {} to {}: {}",
                        from.as_ref().display(),
                        to.as_ref().display(),
                        e
                    ),
                },
            })
    }

    /// Copy a file
    pub async fn copy<P: AsRef<Path>, Q: AsRef<Path>>(&self, from: P, to: Q) -> Result<u64> {
        let from_path = self.resolve_path(&from)?;
        let to_path = self.resolve_path(&to)?;

        // Create parent directory of destination if needed
        if let Some(parent) = to_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| FileSystemError::Io {
                    message: format!(
                        "Failed to create parent directories for {}: {}",
                        to.as_ref().display(),
                        e
                    ),
                })?;
        }

        tokio::fs::copy(&from_path, &to_path)
            .await
            .map_err(|e| match e.kind() {
                io::ErrorKind::NotFound => FileSystemError::NotFound {
                    path: from.as_ref().to_string_lossy().to_string(),
                },
                io::ErrorKind::PermissionDenied => FileSystemError::PermissionDenied {
                    path: from.as_ref().to_string_lossy().to_string(),
                },
                io::ErrorKind::IsADirectory => FileSystemError::IsDirectory {
                    path: from.as_ref().to_string_lossy().to_string(),
                },
                _ => FileSystemError::Io {
                    message: format!(
                        "Failed to copy {} to {}: {}",
                        from.as_ref().display(),
                        to.as_ref().display(),
                        e
                    ),
                },
            })
    }

    /// Check if a path exists
    pub async fn exists<P: AsRef<Path>>(&self, path: P) -> Result<bool> {
        let full_path = self.resolve_path(&path)?;
        Ok(tokio::fs::try_exists(&full_path).await.unwrap_or(false))
    }

    /// Check if a path is a file
    pub async fn is_file<P: AsRef<Path>>(&self, path: P) -> Result<bool> {
        match self.metadata(&path).await {
            Ok(meta) => Ok(meta.is_file),
            Err(FileSystemError::NotFound { .. }) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Check if a path is a directory
    pub async fn is_dir<P: AsRef<Path>>(&self, path: P) -> Result<bool> {
        match self.metadata(&path).await {
            Ok(meta) => Ok(meta.is_dir),
            Err(FileSystemError::NotFound { .. }) => Ok(false),
            Err(e) => Err(e),
        }
    }
}

// Type aliases for backwards compatibility
pub type FileTransferError = FileSystemError;
pub type FileInfo = DirEntry;

/// libunftp compatibility traits and implementations
#[cfg(feature = "libunftp")]
mod libunftp_compat {
    use super::*;

    // Implement libunftp Metadata trait for compatibility
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
            self.modified.ok_or(libunftp::storage::Error::from(
                libunftp::storage::ErrorKind::LocalError,
            ))
        }

        fn gid(&self) -> u32 {
            self.gid
        }

        fn uid(&self) -> u32 {
            self.uid
        }
    }
}
