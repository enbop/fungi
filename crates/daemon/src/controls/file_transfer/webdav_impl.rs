use std::{io::SeekFrom, path::PathBuf, pin::Pin};

use dav_server::{
    davpath::DavPath,
    fs::{
        DavDirEntry, DavFile, DavFileSystem, DavMetaData, FsError, FsFuture, FsResult, FsStream,
        OpenOptions, ReadDirMeta,
    },
};
use futures::{FutureExt, Stream, StreamExt, stream};
use libp2p::bytes::Bytes;

use super::FileTransferClientsControl;

#[derive(Debug, Clone)]
struct DavMetaDataImpl(fungi_fs::Metadata);

impl DavMetaData for DavMetaDataImpl {
    fn len(&self) -> u64 {
        self.0.len
    }

    fn modified(&self) -> dav_server::fs::FsResult<std::time::SystemTime> {
        self.0
            .modified
            .ok_or(dav_server::fs::FsError::NotImplemented)
    }

    fn is_dir(&self) -> bool {
        self.0.is_dir
    }
}

#[derive(Debug)]
struct DavFileImpl {
    path: PathBuf,
    clients_ctrl: FileTransferClientsControl,
    position: u64,
    data: Option<Vec<u8>>,
    options: OpenOptions,
}

impl DavFile for DavFileImpl {
    fn metadata(&mut self) -> FsFuture<Box<dyn DavMetaData>> {
        async move {
            log::info!("DavFile Getting metadata for file: {}", self.path.display());
            let meta = self
                .clients_ctrl
                .metadata(self.path.clone())
                .await
                .map_err(|e| map_error(e, "metadata", &self.path))?;
            Ok(Box::new(DavMetaDataImpl(meta)) as Box<dyn DavMetaData>)
        }
        .boxed()
    }

    fn write_buf(&mut self, buf: Box<dyn libp2p::bytes::Buf + Send>) -> FsFuture<()> {
        async move {
            let bytes = buf.chunk().to_vec();
            log::info!(
                "Writing {} bytes to {} at position {}",
                bytes.len(),
                self.path.display(),
                self.position
            );

            // If this is the first write and we have create permission, ensure file exists
            if self.position == 0 && self.options.create {
                log::debug!("First write to file, ensuring it exists");
            }

            let written = self
                .clients_ctrl
                .put(bytes.clone(), self.path.clone(), self.position)
                .await
                .map_err(|e| map_error(e, "write_buf", &self.path))?;

            log::info!("Successfully wrote {} bytes", written);
            self.position += bytes.len() as u64;
            Ok(())
        }
        .boxed()
    }

    fn write_bytes(&mut self, buf: libp2p::bytes::Bytes) -> FsFuture<()> {
        async move {
            let bytes = buf.to_vec();
            log::info!(
                "Writing {} bytes to {} at position {}",
                bytes.len(),
                self.path.display(),
                self.position
            );

            // If this is the first write and we have create permission, ensure file exists
            if self.position == 0 && self.options.create {
                log::debug!("First write to file, ensuring it exists");
            }

            let written = self
                .clients_ctrl
                .put(bytes, self.path.clone(), self.position)
                .await
                .map_err(|e| map_error(e, "write_bytes", &self.path))?;

            log::info!("Successfully wrote {} bytes", written);
            self.position += buf.len() as u64;
            Ok(())
        }
        .boxed()
    }

    fn read_bytes(&mut self, count: usize) -> FsFuture<libp2p::bytes::Bytes> {
        async move {
            if self.data.is_none() {
                let data = self
                    .clients_ctrl
                    .get(self.path.clone(), self.position)
                    .await
                    .map_err(|e| map_error(e, "read_bytes", &self.path))?;
                self.data = Some(data);
            }

            let data = self.data.as_ref().unwrap();
            let available = data.len();

            if available == 0 {
                return Ok(Bytes::new());
            }

            let to_read = std::cmp::min(count, available);
            let bytes = Bytes::copy_from_slice(&data[..to_read]);

            self.position += to_read as u64;
            if to_read < available {
                self.data = Some(data[to_read..].to_vec());
            } else {
                self.data = None;
            }

            Ok(bytes)
        }
        .boxed()
    }

    fn seek(&mut self, pos: SeekFrom) -> FsFuture<u64> {
        async move {
            match pos {
                SeekFrom::Start(offset) => {
                    self.position = offset;
                    self.data = None;
                }
                SeekFrom::Current(offset) => {
                    if offset >= 0 {
                        self.position += offset as u64;
                    } else {
                        let offset = offset.abs() as u64;
                        if self.position >= offset {
                            self.position -= offset;
                        } else {
                            self.position = 0;
                        }
                    }
                    self.data = None;
                }
                SeekFrom::End(_) => {
                    let meta = self
                        .clients_ctrl
                        .metadata(self.path.clone())
                        .await
                        .map_err(|e| map_error(e, "seek", &self.path))?;

                    if let SeekFrom::End(offset) = pos {
                        if offset >= 0 {
                            self.position = meta.len + offset as u64;
                        } else {
                            let offset = offset.abs() as u64;
                            if meta.len >= offset {
                                self.position = meta.len - offset;
                            } else {
                                self.position = 0;
                            }
                        }
                    }
                    self.data = None;
                }
            }
            Ok(self.position)
        }
        .boxed()
    }

    fn flush(&mut self) -> FsFuture<()> {
        async { Ok(()) }.boxed()
    }
}

struct DavDirEntryImpl {
    name: String,
    metadata: fungi_fs::Metadata,
}

impl DavDirEntry for DavDirEntryImpl {
    fn name(&self) -> Vec<u8> {
        self.name.as_bytes().to_vec()
    }

    fn metadata(&self) -> FsFuture<Box<dyn DavMetaData>> {
        async move {
            let meta = DavMetaDataImpl(self.metadata.clone());
            Ok(Box::new(meta) as Box<dyn DavMetaData>)
        }
        .boxed()
    }
}

impl DavFileSystem for FileTransferClientsControl {
    fn open<'a>(
        &'a self,
        path: &'a DavPath,
        options: OpenOptions,
    ) -> FsFuture<'a, Box<dyn DavFile>> {
        let path_str = path.as_rel_ospath();
        let path_buf = PathBuf::from(path_str.to_string_lossy().to_string());
        let clients_ctrl = self.clone();

        log::info!(
            "Opening file: {} with options: {:?}",
            path_buf.display(),
            options
        );
        async move {
            // Check if file should exist for read-only operations
            if options.read && !options.write && !options.create {
                let _meta = clients_ctrl
                    .metadata(path_buf.clone())
                    .await
                    .map_err(|e| map_error(e, "open", &path_buf))?;
            }

            // For create_new, file must not exist
            if options.create_new {
                match clients_ctrl.metadata(path_buf.clone()).await {
                    Ok(_) => return Err(FsError::Exists),
                    Err(fungi_fs::FileSystemError::NotFound { .. }) => {
                        // File doesn't exist, which is what we want
                    }
                    Err(e) => return Err(map_error(e, "open", &path_buf)),
                }
            }

            // For write operations, we might need to handle file creation
            if options.write && options.create {
                match clients_ctrl.metadata(path_buf.clone()).await {
                    Ok(_) => {
                        log::debug!("File {} exists, will write to it", path_buf.display());
                        // If truncate is requested, truncate the file
                        if options.truncate {
                            log::debug!("Truncating existing file: {}", path_buf.display());
                            let empty_data = Vec::new();
                            clients_ctrl
                                .put(empty_data, path_buf.clone(), 0)
                                .await
                                .map_err(|e| map_error(e, "truncate_file", &path_buf))?;
                        }
                    }
                    Err(fungi_fs::FileSystemError::NotFound { .. }) => {
                        log::debug!(
                            "File {} doesn't exist, will create on first write",
                            path_buf.display()
                        );
                        // For WebDAV, we often need to create the file immediately
                        // especially if it's a PUT request
                        if options.truncate || !options.append {
                            log::debug!("Creating empty file: {}", path_buf.display());
                            let empty_data = Vec::new();
                            clients_ctrl
                                .put(empty_data, path_buf.clone(), 0)
                                .await
                                .map_err(|e| map_error(e, "create_file", &path_buf))?;
                            log::info!("Successfully created empty file: {}", path_buf.display());
                        }
                    }
                    Err(e) => return Err(map_error(e, "open", &path_buf)),
                }
            }

            let file = DavFileImpl {
                path: path_buf,
                clients_ctrl,
                position: if options.truncate { 0 } else { 0 }, // TODO: handle append mode properly
                data: None,
                options,
            };

            Ok(Box::new(file) as Box<dyn DavFile>)
        }
        .boxed()
    }

    fn create_dir<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, ()> {
        let path_buf = PathBuf::from(path.as_rel_ospath());
        let client = self.clone();

        log::info!("Creating directory: {}", path_buf.display());
        async move {
            client
                .mkd(path_buf.clone())
                .await
                .map_err(|e| map_error(e, "create_dir", &path_buf))?;
            Ok(())
        }
        .boxed()
    }

    fn remove_dir<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, ()> {
        let path_buf = PathBuf::from(path.as_rel_ospath());
        let client = self.clone();

        log::info!("Removing directory: {}", path_buf.display());
        async move {
            client
                .rmd(path_buf.clone())
                .await
                .map_err(|e| map_error(e, "remove_dir", &path_buf))?;
            Ok(())
        }
        .boxed()
    }

    fn remove_file<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, ()> {
        let path_buf = PathBuf::from(path.as_rel_ospath());
        let client = self.clone();

        log::info!("Removing file: {}", path_buf.display());
        async move {
            client
                .del(path_buf.clone())
                .await
                .map_err(|e| map_error(e, "remove_file", &path_buf))?;
            Ok(())
        }
        .boxed()
    }

    fn rename<'a>(&'a self, from: &'a DavPath, to: &'a DavPath) -> FsFuture<'a, ()> {
        let from_buf = PathBuf::from(from.as_rel_ospath());
        let to_buf = PathBuf::from(to.as_rel_ospath());
        let client = self.clone();

        log::info!(
            "Renaming from: {} to: {}",
            from_buf.display(),
            to_buf.display()
        );
        async move {
            client
                .rename(from_buf.clone(), to_buf.clone())
                .await
                .map_err(|e| map_error(e, "rename", &from_buf))?;
            Ok(())
        }
        .boxed()
    }

    fn copy<'a>(&'a self, from: &'a DavPath, to: &'a DavPath) -> FsFuture<'a, ()> {
        let from_buf = PathBuf::from(from.as_rel_ospath());
        let to_buf = PathBuf::from(to.as_rel_ospath());
        let client = self.clone();

        log::info!(
            "Copying from: {} to: {}",
            from_buf.display(),
            to_buf.display()
        );
        async move {
            let data = client
                .get(from_buf.clone(), 0)
                .await
                .map_err(|e| map_error(e, "copy (read)", &from_buf))?;

            client
                .put(data, to_buf.clone(), 0)
                .await
                .map_err(|e| map_error(e, "copy (write)", &to_buf))?;

            Ok(())
        }
        .boxed()
    }

    fn read_dir<'a>(
        &'a self,
        path: &'a DavPath,
        _meta: ReadDirMeta,
    ) -> FsFuture<'a, FsStream<Box<dyn dav_server::fs::DavDirEntry>>> {
        let client = self.clone();

        let mut path_buf = PathBuf::from(".");
        path_buf.push(path.as_rel_ospath());

        log::info!("Reading directory: {}", path_buf.display());
        async move {
            let entries = client
                .list(path_buf.clone())
                .await
                .map_err(|e| map_error(e, "read_dir", &path_buf))?;

            let stream = stream::iter(entries).map(|entry| {
                let name = entry.name;

                let dir_entry = DavDirEntryImpl {
                    name,
                    metadata: entry.metadata,
                };

                Ok(Box::new(dir_entry) as Box<dyn DavDirEntry>)
            });

            Ok(Box::pin(stream)
                as Pin<
                    Box<dyn Stream<Item = FsResult<Box<dyn DavDirEntry>>> + Send>,
                >)
        }
        .boxed()
    }

    fn metadata<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, Box<dyn DavMetaData>> {
        let client_ctrl = self.clone();

        let mut path_buf = PathBuf::from(".");
        path_buf.push(path.as_rel_ospath());

        log::info!("Getting metadata for path: {} {}", path, path_buf.display());
        async move {
            let meta = client_ctrl
                .metadata(path_buf.clone())
                .await
                .map_err(|e| map_error(e, "metadata", &path_buf))?;

            Ok(Box::new(DavMetaDataImpl(meta)) as Box<dyn DavMetaData>)
        }
        .boxed()
    }
}

fn map_error(err: fungi_fs::FileSystemError, op: &str, path: &PathBuf) -> FsError {
    use fungi_fs::FileSystemError;
    log::error!(
        "FileSystem error during {}: {} at path: {}",
        op,
        err,
        path.display()
    );
    match err {
        FileSystemError::NotFound { .. } => FsError::NotFound,
        FileSystemError::PermissionDenied { .. } => FsError::Forbidden,
        FileSystemError::AlreadyExists { .. } => FsError::Exists,
        FileSystemError::DirectoryNotEmpty { .. } => FsError::GeneralFailure,
        FileSystemError::IsDirectory { .. } => FsError::GeneralFailure,
        FileSystemError::NotDirectory { .. } => FsError::GeneralFailure,
        FileSystemError::InvalidPath { .. } => FsError::GeneralFailure,
        FileSystemError::NoSpace => FsError::InsufficientStorage,
        FileSystemError::FileTooLarge => FsError::GeneralFailure,
        FileSystemError::ReadOnly => FsError::Forbidden,
        FileSystemError::ConnectionBroken => FsError::GeneralFailure,
        FileSystemError::NotSupported { .. } => FsError::NotImplemented,
        FileSystemError::Io { message } => {
            log::error!("IO error: {}", message);
            FsError::GeneralFailure
        }
        FileSystemError::Other { message } => {
            log::error!("Other error: {}", message);
            FsError::GeneralFailure
        }
    }
}
