use std::{io::SeekFrom, pin::Pin, time::SystemTime};

use dav_server::{
    davpath::DavPath,
    fs::{
        DavDirEntry, DavFile, DavFileSystem, DavMetaData, DavProp, FsError, FsFuture, FsResult,
        FsStream, OpenOptions, ReadDirMeta,
    },
};
use futures::{FutureExt, Stream, StreamExt, future, stream};
use hyper::StatusCode;
use libp2p::bytes::Bytes;
use tarpc::context::Context;
use typed_path::{Utf8Component, Utf8Components, Utf8UnixComponents};

use crate::controls::file_transfer::file_transfer_client::{
    ConnectedClient, convert_string_to_utf8_unix_path_buf,
};

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
    // real path without client prefix
    path_os_string: String,
    client: ConnectedClient,
    position: u64,
    options: OpenOptions,
}

impl DavFile for DavFileImpl {
    fn metadata(&mut self) -> FsFuture<Box<dyn DavMetaData>> {
        log::debug!(
            "DavFile: Getting metadata for file: {}",
            self.path_os_string
        );
        async move {
            let meta = self
                .client
                .metadata(Context::current(), self.path_os_string.clone())
                .await
                .map_err(|_rpc_error| FsError::GeneralFailure)?
                .map_err(|e| map_error(e, "metadata", &self.path_os_string))?;
            Ok(Box::new(DavMetaDataImpl(meta)) as Box<dyn DavMetaData>)
        }
        .boxed()
    }

    fn write_buf(&mut self, buf: Box<dyn libp2p::bytes::Buf + Send>) -> FsFuture<()> {
        let bytes = buf.chunk().to_vec();
        let bytes_len = bytes.len();
        log::debug!(
            "DavFile: Writing {} bytes to file: {} at position: {}",
            bytes_len,
            self.path_os_string,
            self.position
        );
        async move {
            let _written = self
                .client
                .put(
                    Context::current(),
                    bytes,
                    self.path_os_string.clone(),
                    self.position,
                )
                .await
                .map_err(|_rpc_error| FsError::GeneralFailure)?
                .map_err(|e| map_error(e, "write_buf", &self.path_os_string))?;

            self.position += bytes_len as u64;
            Ok(())
        }
        .boxed()
    }

    fn write_bytes(&mut self, buf: libp2p::bytes::Bytes) -> FsFuture<()> {
        let bytes = buf.to_vec();
        log::debug!(
            "DavFile: Writing {} bytes to file: {} at position: {}",
            bytes.len(),
            self.path_os_string,
            self.position
        );
        async move {
            // If this is the first write and we have create permission, ensure file exists
            if self.position == 0 && self.options.create {
                log::debug!("First write to file, ensuring it exists");
            }

            let _written = self
                .client
                .put(
                    Context::current(),
                    bytes,
                    self.path_os_string.clone(),
                    self.position,
                )
                .await
                .map_err(|_rpc_error| FsError::GeneralFailure)?
                .map_err(|e| map_error(e, "write_bytes", &self.path_os_string))?;
            self.position += buf.len() as u64;
            Ok(())
        }
        .boxed()
    }

    fn read_bytes(&mut self, count: usize) -> FsFuture<libp2p::bytes::Bytes> {
        log::debug!(
            "DavFile: Reading {} bytes from file: {} at position: {}",
            count,
            self.path_os_string,
            self.position
        );
        async move {
            let data = self
                .client
                .get_chunk(
                    Context::current(),
                    self.path_os_string.clone(),
                    self.position,
                    count as u64,
                )
                .await
                .map_err(|_rpc_error| FsError::GeneralFailure)?
                .map_err(|e| map_error(e, "read_bytes", &self.path_os_string))?;

            self.position += data.len() as u64;
            Ok(Bytes::from(data))
        }
        .boxed()
    }

    fn seek(&mut self, pos: SeekFrom) -> FsFuture<u64> {
        log::debug!(
            "DavFile: Seeking to position: {:?} in file: {}",
            pos,
            self.path_os_string
        );
        async move {
            match pos {
                SeekFrom::Start(offset) => {
                    self.position = offset;
                }
                SeekFrom::Current(offset) => {
                    if offset >= 0 {
                        self.position += offset as u64;
                    } else {
                        let offset = offset.unsigned_abs();
                        if self.position >= offset {
                            self.position -= offset;
                        } else {
                            self.position = 0;
                        }
                    }
                }
                SeekFrom::End(_) => {
                    let meta = self
                        .client
                        .metadata(Context::current(), self.path_os_string.clone())
                        .await
                        .map_err(|_rpc_error| FsError::GeneralFailure)?
                        .map_err(|e| map_error(e, "seek", &self.path_os_string))?;

                    if let SeekFrom::End(offset) = pos {
                        if offset >= 0 {
                            self.position = meta.len + offset as u64;
                        } else {
                            let offset = offset.unsigned_abs();
                            if meta.len >= offset {
                                self.position = meta.len - offset;
                            } else {
                                self.position = 0;
                            }
                        }
                    }
                }
            }
            Ok(self.position)
        }
        .boxed()
    }

    fn flush(&mut self) -> FsFuture<()> {
        log::debug!("DavFile: Flushing file: {}", self.path_os_string);
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
        log::debug!("DavDirEntry: Getting metadata for entry: {}", self.name);
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
        let path_os_string = path.to_string();
        let clients_ctrl = self.clone();

        log::debug!(
            "DavFileSystem: Opening file: {} with options: {:?}",
            path_os_string,
            options
        );
        async move {
            let unix_path = convert_string_to_utf8_unix_path_buf(&path_os_string).normalize();
            let mut components: Utf8UnixComponents<'_> = unix_path.components();
            let mut client_name = components.next().ok_or_else(|| FsError::GeneralFailure)?;
            // remove the first component if it is root or current
            // "/Test" to "Test"
            if client_name.is_root() || client_name.is_current() {
                client_name = components.next().ok_or_else(|| FsError::GeneralFailure)?;
            }

            let client = clients_ctrl
                .get_client(&client_name.to_string())
                .await
                .map_err(|e| {
                    log::error!("Failed to get file transfer client: {}", e);
                    FsError::GeneralFailure
                })?;
            let real_path_os_string = components.as_str().to_string();

            let meta_res = client
                .metadata(Context::current(), real_path_os_string.clone())
                .await
                .map_err(|_rpc_error| FsError::GeneralFailure)?;

            // For create_new, file must not exist
            if options.create_new {
                if meta_res.is_ok() {
                    return Err(FsError::Exists);
                }
            }

            // For write operations, we might need to handle file creation
            if options.write && options.create {
                if meta_res.is_err() && options.create {
                    log::debug!("File {} doesn't exist, will create it", path_os_string);
                    let empty_data = Vec::new();
                    client
                        .put(
                            Context::current(),
                            empty_data,
                            real_path_os_string.clone(),
                            0,
                        )
                        .await
                        .map_err(|_rpc_error| FsError::GeneralFailure)?
                        .map_err(|e| map_error(e, "create_file", &real_path_os_string))?;
                    log::info!("Successfully created empty file: {}", real_path_os_string);
                }
            }
            let file = DavFileImpl {
                path_os_string: real_path_os_string,
                client,
                position: 0,
                options,
            };

            Ok(Box::new(file) as Box<dyn DavFile>)
        }
        .boxed()
    }

    fn create_dir<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, ()> {
        let path_os_string = path.to_string();
        let client = self.clone();

        log::debug!("DavFileSystem: Creating directory: {}", path_os_string);
        async move {
            client
                .mkd(&path_os_string)
                .await
                .map_err(|e| map_error(e, "create_dir", &path_os_string))?;
            Ok(())
        }
        .boxed()
    }

    fn remove_dir<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, ()> {
        let path_os_string = path.to_string();
        let client = self.clone();

        log::debug!("DavFileSystem: Removing directory: {}", path_os_string);
        async move {
            client
                .rmd(&path_os_string)
                .await
                .map_err(|e| map_error(e, "remove_dir", &path_os_string))?;
            Ok(())
        }
        .boxed()
    }

    fn remove_file<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, ()> {
        let path_os_string = path.to_string();
        let client = self.clone();

        log::debug!("DavFileSystem: Removing file: {}", path_os_string);
        async move {
            client
                .del(&path_os_string)
                .await
                .map_err(|e| map_error(e, "remove_file", &path_os_string))?;
            Ok(())
        }
        .boxed()
    }

    fn rename<'a>(&'a self, from: &'a DavPath, to: &'a DavPath) -> FsFuture<'a, ()> {
        let from_os_string = from.to_string();
        let to_os_string = to.to_string();
        let client = self.clone();

        log::debug!(
            "DavFileSystem: Renaming from: {} to: {}",
            from_os_string,
            to_os_string
        );
        async move {
            client
                .rename(&from_os_string, &to_os_string)
                .await
                .map_err(|e| map_error(e, "rename", &from_os_string))?;
            Ok(())
        }
        .boxed()
    }

    fn copy<'a>(&'a self, from: &'a DavPath, to: &'a DavPath) -> FsFuture<'a, ()> {
        let from_os_string = from.to_string();
        let to_os_string = to.to_string();
        let client = self.clone();

        log::debug!(
            "DavFileSystem:Copying from: {} to: {}",
            from_os_string,
            to_os_string
        );
        async move {
            let data = client
                .get(&from_os_string, 0)
                .await
                .map_err(|e| map_error(e, "copy (read)", &from_os_string))?;

            client
                .put(data, &to_os_string, 0)
                .await
                .map_err(|e| map_error(e, "copy (write)", &to_os_string))?;

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
        let path_os_string = path.to_string();

        log::debug!("DavFileSystem: Reading directory: {}", path_os_string);
        async move {
            let entries = client
                .list(&path_os_string)
                .await
                .map_err(|e| map_error(e, "read_dir", &path_os_string))?;

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
        let path_string = path.to_string();
        log::debug!("DavFileSystem: Getting metadata for path: {path_string}");
        async move {
            let meta = client_ctrl
                .metadata(&path_string)
                .await
                .map_err(|e| map_error(e, "metadata", &path_string))?;

            Ok(Box::new(DavMetaDataImpl(meta)) as Box<dyn DavMetaData>)
        }
        .boxed()
    }

    fn set_accessed<'a>(&'a self, path: &'a DavPath, tm: SystemTime) -> FsFuture<'a, ()> {
        log::debug!(
            "DavFileSystem: Setting accessed time {:?} for path: {}",
            tm,
            path
        );
        async { Ok(()) }.boxed()
    }

    fn set_modified<'a>(&'a self, path: &'a DavPath, tm: SystemTime) -> FsFuture<'a, ()> {
        log::debug!(
            "DavFileSystem: Setting modified time {:?} for path: {}",
            tm,
            path
        );
        async { Ok(()) }.boxed()
    }

    /// Indicator that tells if this filesystem driver supports DAV properties.
    fn have_props<'a>(
        &'a self,
        path: &'a DavPath,
    ) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        log::debug!(
            "DavFileSystem: (have_props) Checking if properties are supported for path: {}",
            path
        );
        Box::pin(future::ready(false))
    }

    /// Patch the DAV properties of a node (add/remove props).
    fn patch_props<'a>(
        &'a self,
        path: &'a DavPath,
        patch: Vec<(bool, DavProp)>,
    ) -> FsFuture<'a, Vec<(StatusCode, DavProp)>> {
        log::debug!(
            "DavFileSystem: (patch_props) Patching properties for path: {} with patch: {:?}",
            path,
            patch
        );
        // TODO: Implement property patching
        async { Ok(Vec::new()) }.boxed()
    }

    /// List/get the DAV properties of a node.
    fn get_props<'a>(&'a self, path: &'a DavPath, do_content: bool) -> FsFuture<'a, Vec<DavProp>> {
        log::debug!(
            "DavFileSystem: (get_props) Getting properties for path: {} with content: {}",
            path,
            do_content
        );
        // TODO: Implement property retrieval
        async { Ok(Vec::new()) }.boxed()
    }

    /// Get one specific named property of a node.
    fn get_prop<'a>(&'a self, path: &'a DavPath, prop: DavProp) -> FsFuture<'a, Vec<u8>> {
        log::debug!(
            "DavFileSystem: (get_prop) Getting property {:?} for path: {}",
            prop,
            path
        );
        // TODO: Implement specific property retrieval
        async { Ok(Vec::new()) }.boxed()
    }

    /// Get quota of this filesystem (used/total space).
    ///
    /// The first value returned is the amount of space used,
    /// the second optional value is the total amount of space
    /// (used + available).
    fn get_quota(&self) -> FsFuture<(u64, Option<u64>)> {
        log::debug!("DavFileSystem: (get_quota) Getting filesystem quota");
        async { Ok((0, None)) }.boxed()
    }
}

fn map_error(err: fungi_fs::FileSystemError, op: &str, path: &str) -> FsError {
    use fungi_fs::FileSystemError;
    log::error!("FileSystem error during {}: {} at path: {}", op, err, path);
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
            log::error!("IO error: {message}");
            FsError::GeneralFailure
        }
        FileSystemError::Other { message } => {
            log::error!("Other error: {message}");
            FsError::GeneralFailure
        }
    }
}
