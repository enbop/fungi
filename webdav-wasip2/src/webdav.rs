use std::{io::SeekFrom, pin::Pin, time::SystemTime};

use bytes::{Buf, Bytes};
use dav_server::{
    davpath::DavPath,
    fs::{
        DavDirEntry, DavFile, DavFileSystem, DavMetaData, DavProp, FsError, FsFuture, FsResult,
        FsStream, OpenOptions, ReadDirMeta,
    },
};
use futures::{FutureExt, Stream, future, stream};
use hyper::StatusCode;

use crate::{BackendError, DirEntry, Metadata, WebDavBackend, normalize_path};

#[derive(Debug, Clone)]
struct DavMetaDataImpl(Metadata);

impl DavMetaData for DavMetaDataImpl {
    fn len(&self) -> u64 {
        self.0.len
    }

    fn modified(&self) -> FsResult<SystemTime> {
        self.0.modified.ok_or(FsError::NotImplemented)
    }

    fn is_dir(&self) -> bool {
        self.0.is_dir
    }

    fn is_file(&self) -> bool {
        self.0.is_file
    }

    fn is_symlink(&self) -> bool {
        self.0.is_symlink
    }

    fn created(&self) -> FsResult<SystemTime> {
        self.0.created.ok_or(FsError::NotImplemented)
    }

    fn accessed(&self) -> FsResult<SystemTime> {
        self.0.accessed.ok_or(FsError::NotImplemented)
    }

    fn status_changed(&self) -> FsResult<SystemTime> {
        self.0.modified.ok_or(FsError::NotImplemented)
    }

    fn executable(&self) -> FsResult<bool> {
        Ok((self.0.permissions & 0o111) != 0)
    }

    fn etag(&self) -> Option<String> {
        use std::time::UNIX_EPOCH;

        let modified = self.0.modified?;
        let duration = modified.duration_since(UNIX_EPOCH).ok()?;
        let timestamp_us =
            duration.as_secs() * 1_000_000 + u64::from(duration.subsec_nanos()) / 1000;
        if self.0.is_file && self.0.len > 0 {
            Some(format!("{:x}-{:x}", self.0.len, timestamp_us))
        } else {
            Some(format!("{:x}", timestamp_us))
        }
    }
}

struct DavFileImpl<B> {
    path: String,
    backend: B,
    position: u64,
    write_buffer: Vec<u8>,
    buffer_size: usize,
    buffer_start_position: u64,
}

impl<B> std::fmt::Debug for DavFileImpl<B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DavFileImpl")
            .field("path", &self.path)
            .field("position", &self.position)
            .field("write_buffer_len", &self.write_buffer.len())
            .field("buffer_size", &self.buffer_size)
            .finish()
    }
}

impl<B> DavFileImpl<B>
where
    B: WebDavBackend,
{
    fn flush_buffer(&mut self) -> FsFuture<'_, ()> {
        if self.write_buffer.is_empty() {
            return async { Ok(()) }.boxed();
        }

        let bytes = std::mem::take(&mut self.write_buffer);
        let position = self.buffer_start_position;
        let path = self.path.clone();
        let backend = self.backend.clone();
        self.buffer_start_position = self.position;

        async move {
            backend
                .write_chunk(&path, position, bytes)
                .await
                .map_err(map_error)?;
            Ok(())
        }
        .boxed()
    }

    fn write_chunk(&mut self, chunk: Vec<u8>) -> FsFuture<'_, ()> {
        let len = chunk.len();
        if self.write_buffer.is_empty() {
            self.buffer_start_position = self.position;
        }

        if !self.write_buffer.is_empty() && self.write_buffer.len() + len >= self.buffer_size {
            let path = self.path.clone();
            let backend = self.backend.clone();
            let flushed = std::mem::take(&mut self.write_buffer);
            let flush_position = self.buffer_start_position;

            self.buffer_start_position = self.position;
            self.write_buffer.extend(chunk);
            self.position += len as u64;

            async move {
                backend
                    .write_chunk(&path, flush_position, flushed)
                    .await
                    .map_err(map_error)?;
                Ok(())
            }
            .boxed()
        } else {
            self.write_buffer.extend(chunk);
            self.position += len as u64;
            if self.write_buffer.len() >= self.buffer_size {
                self.flush_buffer()
            } else {
                async { Ok(()) }.boxed()
            }
        }
    }
}

impl<B> DavFile for DavFileImpl<B>
where
    B: WebDavBackend,
{
    fn metadata(&mut self) -> FsFuture<'_, Box<dyn DavMetaData>> {
        let backend = self.backend.clone();
        let path = self.path.clone();
        async move {
            let metadata = backend.metadata(&path).await.map_err(map_error)?;
            Ok(Box::new(DavMetaDataImpl(metadata)) as Box<dyn DavMetaData>)
        }
        .boxed()
    }

    fn write_buf(&mut self, buf: Box<dyn Buf + Send>) -> FsFuture<'_, ()> {
        self.write_chunk(buf.chunk().to_vec())
    }

    fn write_bytes(&mut self, buf: Bytes) -> FsFuture<'_, ()> {
        self.write_chunk(buf.to_vec())
    }

    fn read_bytes(&mut self, count: usize) -> FsFuture<'_, Bytes> {
        let backend = self.backend.clone();
        let path = self.path.clone();
        let position = self.position;
        async move {
            let data = backend
                .read_chunk(&path, position, count as u64)
                .await
                .map_err(map_error)?;
            self.position += data.len() as u64;
            Ok(Bytes::from(data))
        }
        .boxed()
    }

    fn seek(&mut self, pos: SeekFrom) -> FsFuture<'_, u64> {
        if !self.write_buffer.is_empty() {
            return self.flush_then_seek(pos);
        }

        let backend = self.backend.clone();
        let path = self.path.clone();
        async move {
            self.position = resolve_seek(backend, &path, self.position, pos).await?;
            self.buffer_start_position = self.position;
            Ok(self.position)
        }
        .boxed()
    }

    fn flush(&mut self) -> FsFuture<'_, ()> {
        self.flush_buffer()
    }
}

impl<B> DavFileImpl<B>
where
    B: WebDavBackend,
{
    fn flush_then_seek(&mut self, pos: SeekFrom) -> FsFuture<'_, u64> {
        let backend = self.backend.clone();
        let path = self.path.clone();
        let flushed = std::mem::take(&mut self.write_buffer);
        let flush_position = self.buffer_start_position;
        async move {
            backend
                .write_chunk(&path, flush_position, flushed)
                .await
                .map_err(map_error)?;
            self.position = resolve_seek(backend, &path, self.position, pos).await?;
            self.buffer_start_position = self.position;
            Ok(self.position)
        }
        .boxed()
    }
}

struct DavDirEntryImpl {
    entry: DirEntry,
}

impl DavDirEntry for DavDirEntryImpl {
    fn name(&self) -> Vec<u8> {
        self.entry.name.as_bytes().to_vec()
    }

    fn metadata(&self) -> FsFuture<'_, Box<dyn DavMetaData>> {
        let metadata = self.entry.metadata.clone();
        async move { Ok(Box::new(DavMetaDataImpl(metadata)) as Box<dyn DavMetaData>) }.boxed()
    }

    fn is_dir(&self) -> FsFuture<'_, bool> {
        let is_dir = self.entry.metadata.is_dir;
        async move { Ok(is_dir) }.boxed()
    }

    fn is_file(&self) -> FsFuture<'_, bool> {
        let is_file = self.entry.metadata.is_file;
        async move { Ok(is_file) }.boxed()
    }

    fn is_symlink(&self) -> FsFuture<'_, bool> {
        let is_symlink = self.entry.metadata.is_symlink;
        async move { Ok(is_symlink) }.boxed()
    }
}

#[derive(Debug, Clone)]
pub struct WebDavFileSystem<B> {
    backend: B,
    write_buffer_size: usize,
}

impl<B> WebDavFileSystem<B>
where
    B: WebDavBackend,
{
    pub const DEFAULT_WRITE_BUFFER_SIZE: usize = 1024 * 1024;

    pub fn new(backend: B) -> Self {
        Self::new_with_buffer_size(backend, Self::DEFAULT_WRITE_BUFFER_SIZE)
    }

    pub fn new_with_buffer_size(backend: B, write_buffer_size: usize) -> Self {
        Self {
            backend,
            write_buffer_size,
        }
    }
}

impl<B> DavFileSystem for WebDavFileSystem<B>
where
    B: WebDavBackend,
{
    fn open<'a>(
        &'a self,
        path: &'a DavPath,
        options: OpenOptions,
    ) -> FsFuture<'a, Box<dyn DavFile>> {
        let backend = self.backend.clone();
        let buffer_size = self.write_buffer_size;
        let path = path.to_string();
        async move {
            let normalized = normalize_path(&path).map_err(map_error)?;
            let metadata = backend.metadata(&normalized).await;

            if options.create_new && metadata.is_ok() {
                return Err(FsError::Exists);
            }

            if options.write
                && options.create
                && matches!(metadata, Err(BackendError::NotFound { .. }))
            {
                backend
                    .write_chunk(&normalized, 0, Vec::new())
                    .await
                    .map_err(map_error)?;
            } else {
                metadata.map_err(map_error)?;
            }

            Ok(Box::new(DavFileImpl {
                path: normalized,
                backend,
                position: 0,
                write_buffer: Vec::new(),
                buffer_size,
                buffer_start_position: 0,
            }) as Box<dyn DavFile>)
        }
        .boxed()
    }

    fn create_dir<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, ()> {
        let backend = self.backend.clone();
        let path = path.to_string();
        async move {
            let normalized = normalize_path(&path).map_err(map_error)?;
            backend.create_dir(&normalized).await.map_err(map_error)
        }
        .boxed()
    }

    fn remove_dir<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, ()> {
        let backend = self.backend.clone();
        let path = path.to_string();
        async move {
            let normalized = normalize_path(&path).map_err(map_error)?;
            backend.remove_dir(&normalized).await.map_err(map_error)
        }
        .boxed()
    }

    fn remove_file<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, ()> {
        let backend = self.backend.clone();
        let path = path.to_string();
        async move {
            let normalized = normalize_path(&path).map_err(map_error)?;
            backend.remove_file(&normalized).await.map_err(map_error)
        }
        .boxed()
    }

    fn rename<'a>(&'a self, from: &'a DavPath, to: &'a DavPath) -> FsFuture<'a, ()> {
        let backend = self.backend.clone();
        let from = from.to_string();
        let to = to.to_string();
        async move {
            let from = normalize_path(&from).map_err(map_error)?;
            let to = normalize_path(&to).map_err(map_error)?;
            backend.rename(&from, &to).await.map_err(map_error)
        }
        .boxed()
    }

    fn copy<'a>(&'a self, from: &'a DavPath, to: &'a DavPath) -> FsFuture<'a, ()> {
        let backend = self.backend.clone();
        let from = from.to_string();
        let to = to.to_string();
        async move {
            let from = normalize_path(&from).map_err(map_error)?;
            let to = normalize_path(&to).map_err(map_error)?;
            backend.copy(&from, &to).await.map_err(map_error)
        }
        .boxed()
    }

    fn read_dir<'a>(
        &'a self,
        path: &'a DavPath,
        _meta: ReadDirMeta,
    ) -> FsFuture<'a, FsStream<Box<dyn DavDirEntry>>> {
        let backend = self.backend.clone();
        let path = path.to_string();
        async move {
            let normalized = normalize_path(&path).map_err(map_error)?;
            let entries = backend.read_dir(&normalized).await.map_err(map_error)?;
            let stream = stream::iter(
                entries
                    .into_iter()
                    .map(|entry| Ok(Box::new(DavDirEntryImpl { entry }) as Box<dyn DavDirEntry>)),
            );
            Ok(Box::pin(stream)
                as Pin<
                    Box<dyn Stream<Item = FsResult<Box<dyn DavDirEntry>>> + Send>,
                >)
        }
        .boxed()
    }

    fn metadata<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, Box<dyn DavMetaData>> {
        let backend = self.backend.clone();
        let path = path.to_string();
        async move {
            let normalized = normalize_path(&path).map_err(map_error)?;
            let metadata = backend.metadata(&normalized).await.map_err(map_error)?;
            Ok(Box::new(DavMetaDataImpl(metadata)) as Box<dyn DavMetaData>)
        }
        .boxed()
    }

    fn set_accessed<'a>(&'a self, _path: &'a DavPath, _tm: SystemTime) -> FsFuture<'a, ()> {
        async { Ok(()) }.boxed()
    }

    fn set_modified<'a>(&'a self, _path: &'a DavPath, _tm: SystemTime) -> FsFuture<'a, ()> {
        async { Ok(()) }.boxed()
    }

    fn have_props<'a>(
        &'a self,
        _path: &'a DavPath,
    ) -> Pin<Box<dyn futures::Future<Output = bool> + Send + 'a>> {
        Box::pin(future::ready(false))
    }

    fn patch_props<'a>(
        &'a self,
        _path: &'a DavPath,
        _patch: Vec<(bool, DavProp)>,
    ) -> FsFuture<'a, Vec<(StatusCode, DavProp)>> {
        async { Ok(Vec::new()) }.boxed()
    }

    fn get_props<'a>(
        &'a self,
        _path: &'a DavPath,
        _do_content: bool,
    ) -> FsFuture<'a, Vec<DavProp>> {
        async { Ok(Vec::new()) }.boxed()
    }

    fn get_prop<'a>(&'a self, _path: &'a DavPath, _prop: DavProp) -> FsFuture<'a, Vec<u8>> {
        async { Ok(Vec::new()) }.boxed()
    }

    fn get_quota(&self) -> FsFuture<'_, (u64, Option<u64>)> {
        async { Ok((0, None)) }.boxed()
    }
}

async fn resolve_seek<B>(backend: B, path: &str, current: u64, pos: SeekFrom) -> FsResult<u64>
where
    B: WebDavBackend,
{
    let target = match pos {
        SeekFrom::Start(offset) => offset,
        SeekFrom::Current(offset) => apply_signed_offset(current, offset),
        SeekFrom::End(offset) => {
            let metadata = backend.metadata(path).await.map_err(map_error)?;
            apply_signed_offset(metadata.len, offset)
        }
    };
    Ok(target)
}

fn apply_signed_offset(base: u64, offset: i64) -> u64 {
    if offset >= 0 {
        base.saturating_add(offset as u64)
    } else {
        base.saturating_sub(offset.unsigned_abs())
    }
}

fn map_error(error: BackendError) -> FsError {
    match error {
        BackendError::NotFound { .. } => FsError::NotFound,
        BackendError::PermissionDenied { .. } => FsError::Forbidden,
        BackendError::AlreadyExists { .. } => FsError::Exists,
        BackendError::DirectoryNotEmpty { .. } => FsError::GeneralFailure,
        BackendError::IsDirectory { .. } => FsError::GeneralFailure,
        BackendError::NotDirectory { .. } => FsError::GeneralFailure,
        BackendError::InvalidPath { .. } => FsError::GeneralFailure,
        BackendError::NoSpace => FsError::InsufficientStorage,
        BackendError::FileTooLarge => FsError::GeneralFailure,
        BackendError::ReadOnly => FsError::Forbidden,
        BackendError::NotSupported { .. } => FsError::NotImplemented,
        BackendError::Other { .. } => FsError::GeneralFailure,
    }
}
