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
        let path = path.as_ref().to_path_buf();
        self.metadata(path).await.map_err(map_error)
    }

    async fn list<P: AsRef<std::path::Path> + Send + Debug>(
        &self,
        _user: &User,
        path: P,
    ) -> libunftp::storage::Result<
        Vec<libunftp::storage::Fileinfo<std::path::PathBuf, Self::Metadata>>,
    > {
        let path = path.as_ref().to_path_buf();
        let file_infos = self.list(path).await.map_err(map_error)?;

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
        let path = path.as_ref().to_path_buf();
        let bytes = self.get(path, start_pos).await.map_err(map_error)?;

        let cursor = std::io::Cursor::new(bytes);
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
        let path = path.as_ref().to_path_buf();

        let mut buffer = Vec::new();
        tokio::io::copy(&mut bytes, &mut buffer)
            .await
            .map_err(|e| {
                libunftp::storage::Error::new(libunftp::storage::ErrorKind::LocalError, e)
            })?;

        self.put(buffer, path, start_pos).await.map_err(map_error)
    }

    async fn del<P: AsRef<std::path::Path> + Send + Debug>(
        &self,
        _user: &User,
        path: P,
    ) -> libunftp::storage::Result<()> {
        let path = path.as_ref().to_path_buf();
        self.del(path).await.map_err(map_error)
    }

    async fn rmd<P: AsRef<std::path::Path> + Send + Debug>(
        &self,
        _user: &User,
        path: P,
    ) -> libunftp::storage::Result<()> {
        let path = path.as_ref().to_path_buf();
        self.rmd(path).await.map_err(map_error)
    }

    async fn mkd<P: AsRef<std::path::Path> + Send + Debug>(
        &self,
        _user: &User,
        path: P,
    ) -> libunftp::storage::Result<()> {
        let path = path.as_ref().to_path_buf();
        self.mkd(path).await.map_err(map_error)
    }

    async fn rename<P: AsRef<std::path::Path> + Send + Debug>(
        &self,
        _user: &User,
        from: P,
        to: P,
    ) -> libunftp::storage::Result<()> {
        let from = from.as_ref().to_path_buf();
        let to = to.as_ref().to_path_buf();
        self.rename(from, to).await.map_err(map_error)
    }

    async fn cwd<P: AsRef<std::path::Path> + Send + Debug>(
        &self,
        _user: &User,
        path: P,
    ) -> libunftp::storage::Result<()> {
        let path = path.as_ref().to_path_buf();
        self.cwd(path).await.map_err(map_error)
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
