use std::fmt::Debug;

use async_trait::async_trait;
use libunftp::{auth::UserDetail, storage::StorageBackend};

use super::FileTransferClientControl;

#[async_trait]
impl<User: UserDetail> StorageBackend<User> for FileTransferClientControl {
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
        self.metadata(path).await.map_err(|e| map_error(e))
    }

    async fn list<P: AsRef<std::path::Path> + Send + Debug>(
        &self,
        _user: &User,
        path: P,
    ) -> libunftp::storage::Result<
        Vec<libunftp::storage::Fileinfo<std::path::PathBuf, Self::Metadata>>,
    > {
        let path = path.as_ref().to_path_buf();
        let file_infos = self.list(path).await.map_err(|e| map_error(e))?;

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
        let bytes = self.get(path, start_pos).await.map_err(|e| map_error(e))?;

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

        self.put(buffer, path, start_pos)
            .await
            .map_err(|e| map_error(e))
    }

    async fn del<P: AsRef<std::path::Path> + Send + Debug>(
        &self,
        _user: &User,
        path: P,
    ) -> libunftp::storage::Result<()> {
        let path = path.as_ref().to_path_buf();
        self.del(path).await.map_err(|e| map_error(e))
    }

    async fn rmd<P: AsRef<std::path::Path> + Send + Debug>(
        &self,
        _user: &User,
        path: P,
    ) -> libunftp::storage::Result<()> {
        let path = path.as_ref().to_path_buf();
        self.rmd(path).await.map_err(|e| map_error(e))
    }

    async fn mkd<P: AsRef<std::path::Path> + Send + Debug>(
        &self,
        _user: &User,
        path: P,
    ) -> libunftp::storage::Result<()> {
        let path = path.as_ref().to_path_buf();
        self.mkd(path).await.map_err(|e| map_error(e))
    }

    async fn rename<P: AsRef<std::path::Path> + Send + Debug>(
        &self,
        _user: &User,
        from: P,
        to: P,
    ) -> libunftp::storage::Result<()> {
        let from = from.as_ref().to_path_buf();
        let to = to.as_ref().to_path_buf();
        self.rename(from, to).await.map_err(|e| map_error(e))
    }

    async fn cwd<P: AsRef<std::path::Path> + Send + Debug>(
        &self,
        _user: &User,
        path: P,
    ) -> libunftp::storage::Result<()> {
        let path = path.as_ref().to_path_buf();
        self.cwd(path).await.map_err(|e| map_error(e))
    }
}

fn map_error(err: fungi_fs::FileTransferError) -> libunftp::storage::Error {
    use fungi_fs::FileTransferError;
    use libunftp::storage::ErrorKind;

    match err {
        FileTransferError::NotFound => ErrorKind::PermanentFileNotAvailable.into(),
        FileTransferError::PermissionDenied => ErrorKind::PermissionDenied.into(),
        FileTransferError::ConnectionBroken => ErrorKind::ConnectionClosed.into(),
        FileTransferError::Other(msg) => {
            log::error!("File transfer error: {}", msg);
            ErrorKind::LocalError.into()
        }
    }
}
