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
use tarpc::context;

use super::FileTransferRpcClient;

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
    client: FileTransferRpcClient,
    position: u64,
    data: Option<Vec<u8>>,
    options: OpenOptions,
}

// TODO: handle errors properly

impl DavFile for DavFileImpl {
    fn metadata(&mut self) -> FsFuture<Box<dyn DavMetaData>> {
        async move {
            let meta = self
                .client
                .metadata(context::current(), self.path.clone())
                .await
                .unwrap()
                .unwrap();
            Ok(Box::new(DavMetaDataImpl(meta)) as Box<dyn DavMetaData>)
        }
        .boxed()
    }

    fn write_buf(&mut self, buf: Box<dyn libp2p::bytes::Buf + Send>) -> FsFuture<()> {
        async move {
            let bytes = buf.chunk().to_vec();
            let _ = self
                .client
                .put(
                    context::current(),
                    bytes.clone(),
                    self.path.clone(),
                    self.position,
                )
                .await
                .unwrap()
                .unwrap();
            self.position += bytes.len() as u64;
            Ok(())
        }
        .boxed()
    }

    fn write_bytes(&mut self, buf: libp2p::bytes::Bytes) -> FsFuture<()> {
        async move {
            let bytes = buf.to_vec();
            let _ = self
                .client
                .put(context::current(), bytes, self.path.clone(), self.position)
                .await
                .unwrap()
                .unwrap();
            self.position += buf.len() as u64;
            Ok(())
        }
        .boxed()
    }

    fn read_bytes(&mut self, count: usize) -> FsFuture<libp2p::bytes::Bytes> {
        async move {
            // TODO don't clone
            if self.data.is_none() {
                let data = self
                    .client
                    .get(context::current(), self.path.clone(), self.position)
                    .await
                    .unwrap()
                    .unwrap();
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
                        .client
                        .metadata(context::current(), self.path.clone())
                        .await
                        .unwrap()
                        .unwrap();

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

impl DavFileSystem for FileTransferRpcClient {
    fn open<'a>(
        &'a self,
        path: &'a DavPath,
        options: OpenOptions,
    ) -> FsFuture<'a, Box<dyn DavFile>> {
        let path_str = path.as_rel_ospath();
        let path_buf = PathBuf::from(path_str.to_string_lossy().to_string());
        let client = self.clone();

        log::info!(
            "Opening file: {} with options: {:?}",
            path_buf.display(),
            options
        );
        async move {
            if !options.write && !options.create {
                let meta_result = client.metadata(context::current(), path_buf.clone()).await;

                if let Err(_) = meta_result {
                    return Err(FsError::NotFound);
                }
            }

            let file = DavFileImpl {
                path: path_buf,
                client,
                position: 0,
                data: None,
                options,
            };

            Ok(Box::new(file) as Box<dyn DavFile>)
        }
        .boxed()
    }

    fn read_dir<'a>(
        &'a self,
        path: &'a DavPath,
        meta: ReadDirMeta,
    ) -> FsFuture<'a, FsStream<Box<dyn dav_server::fs::DavDirEntry>>> {
        let client = self.clone();

        let mut path_buf = PathBuf::from(".");
        path_buf.push(path.as_rel_ospath());

        log::info!("Reading directory: {}", path_buf.display());
        async move {
            let entries = client
                .list(context::current(), path_buf.clone())
                .await
                .map_err(|_| FsError::NotFound)?
                .map_err(|_| FsError::NotFound)?;

            let stream = stream::iter(entries).map(|entry| {
                let name = entry
                    .path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();

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
        let client = self.clone();

        let mut path_buf = PathBuf::from(".");
        path_buf.push(path.as_rel_ospath());

        log::info!("Getting metadata for path: {} {}", path, path_buf.display());
        async move {
            let meta = client
                .metadata(context::current(), path_buf)
                .await
                .unwrap()
                .map_err(|_| FsError::NotFound)?;

            Ok(Box::new(DavMetaDataImpl(meta)) as Box<dyn DavMetaData>)
        }
        .boxed()
    }
}
