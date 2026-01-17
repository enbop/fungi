use std::fmt::Debug;

use async_trait::async_trait;
use libunftp::{auth::UserDetail, storage::StorageBackend};

use super::FileTransferClientsControl;

#[async_trait]
impl<User: UserDetail> StorageBackend<User> for FileTransferClientsControl {
    type Metadata = fungi_fs::Metadata;

    fn supported_features(&self) -> u32 {
        libunftp::storage::FEATURE_RESTART
    }

    async fn metadata<P: AsRef<std::path::Path> + Send + Debug>(
        &self,
        _user: &User,
        path: P,
    ) -> libunftp::storage::Result<Self::Metadata> {
        let path_str = path.as_ref().to_string_lossy().to_string();
        log::debug!("FTP: Getting metadata for path: {}", path_str);

        self.metadata(&path_str).await.map_err(map_error)
    }

    async fn list<P: AsRef<std::path::Path> + Send + Debug>(
        &self,
        _user: &User,
        path: P,
    ) -> libunftp::storage::Result<
        Vec<libunftp::storage::Fileinfo<std::path::PathBuf, Self::Metadata>>,
    > {
        let path_str = path.as_ref().to_string_lossy().to_string();
        log::debug!("FTP: Listing directory: {}", path_str);

        let file_infos = self.list(&path_str).await.map_err(map_error)?;

        Ok(file_infos
            .into_iter()
            .map(|info| libunftp::storage::Fileinfo {
                path: info.path,
                metadata: info.metadata,
            })
            .collect())
    }

    async fn get<P: AsRef<std::path::Path> + Send + Debug>(
        &self,
        _user: &User,
        path: P,
        start_pos: u64,
    ) -> libunftp::storage::Result<Box<dyn tokio::io::AsyncRead + Send + Sync + Unpin>> {
        let path_str = path.as_ref().to_string_lossy().to_string();
        log::debug!(
            "FTP: Reading file: {} from position: {}",
            path_str,
            start_pos
        );

        // TODO find a better way to impl AsyncRead
        // Read the entire file content in chunks
        let mut all_data = Vec::new();
        let mut current_pos = start_pos;
        const CHUNK_SIZE: u64 = 64 * 1024; // 64KB chunks
        let mut chunk_count = 0;

        loop {
            log::debug!(
                "FTP: Reading chunk #{} at position {} with size {}",
                chunk_count,
                current_pos,
                CHUNK_SIZE
            );

            let chunk = self
                .get_chunk(&path_str, current_pos, CHUNK_SIZE)
                .await
                .map_err(map_error)?;

            if chunk.is_empty() {
                log::debug!(
                    "FTP: Reached end of file after {} chunks, total bytes read: {}",
                    chunk_count,
                    all_data.len()
                );
                break;
            }

            log::debug!(
                "FTP: Successfully read chunk #{} with {} bytes at position {}",
                chunk_count,
                chunk.len(),
                current_pos
            );
            current_pos += chunk.len() as u64;
            all_data.extend(chunk);
            chunk_count += 1;

            // Add a small yield to allow other tasks to run
            tokio::task::yield_now().await;
        }

        log::info!(
            "FTP: Completed reading file: {} - {} bytes in {} chunks",
            path_str,
            all_data.len(),
            chunk_count
        );

        let cursor = std::io::Cursor::new(all_data);
        Ok(Box::new(cursor) as Box<dyn tokio::io::AsyncRead + Send + Sync + Unpin>)
    }

    async fn put<
        P: AsRef<std::path::Path> + Send,
        R: tokio::io::AsyncRead + Send + Sync + 'static + Unpin,
    >(
        &self,
        _user: &User,
        mut bytes: R,
        path: P,
        start_pos: u64,
    ) -> libunftp::storage::Result<u64> {
        let path_str = path.as_ref().to_string_lossy().to_string();
        log::debug!(
            "FTP: Writing to file: {} at position: {}",
            path_str,
            start_pos
        );

        let mut buffer = Vec::new();
        tokio::io::copy(&mut bytes, &mut buffer)
            .await
            .map_err(|e| {
                libunftp::storage::Error::new(libunftp::storage::ErrorKind::LocalError, e)
            })?;

        log::debug!("FTP: Writing {} bytes to file: {}", buffer.len(), path_str);
        self.put(buffer, &path_str, start_pos)
            .await
            .map_err(map_error)
    }

    async fn del<P: AsRef<std::path::Path> + Send + Debug>(
        &self,
        _user: &User,
        path: P,
    ) -> libunftp::storage::Result<()> {
        let path_str = path.as_ref().to_string_lossy().to_string();
        log::debug!("FTP: Deleting file: {}", path_str);

        self.del(&path_str).await.map_err(map_error)
    }

    async fn rmd<P: AsRef<std::path::Path> + Send + Debug>(
        &self,
        _user: &User,
        path: P,
    ) -> libunftp::storage::Result<()> {
        let path_str = path.as_ref().to_string_lossy().to_string();
        log::debug!("FTP: Removing directory: {}", path_str);

        self.rmd(&path_str).await.map_err(map_error)
    }

    async fn mkd<P: AsRef<std::path::Path> + Send + Debug>(
        &self,
        _user: &User,
        path: P,
    ) -> libunftp::storage::Result<()> {
        let path_str = path.as_ref().to_string_lossy().to_string();
        log::debug!("FTP: Creating directory: {}", path_str);

        self.mkd(&path_str).await.map_err(map_error)
    }

    async fn rename<P: AsRef<std::path::Path> + Send + Debug>(
        &self,
        _user: &User,
        from: P,
        to: P,
    ) -> libunftp::storage::Result<()> {
        let from_str = from.as_ref().to_string_lossy().to_string();
        let to_str = to.as_ref().to_string_lossy().to_string();
        log::debug!("FTP: Renaming from: {} to: {}", from_str, to_str);

        self.rename(&from_str, &to_str).await.map_err(map_error)
    }

    async fn cwd<P: AsRef<std::path::Path> + Send + Debug>(
        &self,
        _user: &User,
        path: P,
    ) -> libunftp::storage::Result<()> {
        let path_str = path.as_ref().to_string_lossy().to_string();
        log::debug!("FTP: Changing working directory to: {}", path_str);

        self.cwd(&path_str).await.map_err(map_error)
    }
}

fn map_error(err: fungi_fs::FileSystemError) -> libunftp::storage::Error {
    use fungi_fs::FileSystemError;
    use libunftp::storage::ErrorKind;

    match err {
        FileSystemError::NotFound { .. } => ErrorKind::PermanentFileNotAvailable.into(),
        FileSystemError::PermissionDenied { .. } => ErrorKind::PermissionDenied.into(),
        FileSystemError::ConnectionBroken => ErrorKind::ConnectionClosed.into(),
        FileSystemError::AlreadyExists { .. } => ErrorKind::LocalError.into(), // No direct equivalent
        FileSystemError::DirectoryNotEmpty { .. } => ErrorKind::TransientFileNotAvailable.into(),
        FileSystemError::IsDirectory { .. } => ErrorKind::TransientFileNotAvailable.into(),
        FileSystemError::NotDirectory { .. } => ErrorKind::TransientFileNotAvailable.into(),
        FileSystemError::InvalidPath { .. } => ErrorKind::LocalError.into(), // No direct equivalent
        FileSystemError::NoSpace => ErrorKind::InsufficientStorageSpaceError.into(),
        FileSystemError::FileTooLarge => ErrorKind::LocalError.into(), // No direct equivalent
        FileSystemError::ReadOnly => ErrorKind::PermissionDenied.into(),
        FileSystemError::NotSupported { .. } => ErrorKind::CommandNotImplemented.into(),
        FileSystemError::Io { message } | FileSystemError::Other { message } => {
            log::error!("File transfer error: {message}");
            ErrorKind::LocalError.into()
        }
    }
}
