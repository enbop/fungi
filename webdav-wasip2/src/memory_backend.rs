use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;

use crate::{BackendError, DirEntry, Metadata, Result, WebDavBackend, normalize_path};

#[derive(Debug, Clone)]
pub struct MemoryBackend {
    inner: Arc<Mutex<State>>,
}

#[derive(Debug, Default)]
struct State {
    entries: BTreeMap<String, Node>,
}

#[derive(Debug, Clone)]
struct Node {
    metadata: Metadata,
    kind: NodeKind,
}

#[derive(Debug, Clone)]
enum NodeKind {
    File(Vec<u8>),
    Dir,
}

impl Default for MemoryBackend {
    fn default() -> Self {
        let mut entries = BTreeMap::new();
        entries.insert(
            String::new(),
            Node {
                metadata: Metadata::directory(),
                kind: NodeKind::Dir,
            },
        );
        Self {
            inner: Arc::new(Mutex::new(State { entries })),
        }
    }
}

impl MemoryBackend {
    pub fn demo() -> Self {
        let backend = Self::default();
        backend.create_dir_sync("docs");
        backend.write_file_sync(
            "hello.txt",
            b"hello from webdav-wasip2
",
        );
        backend.write_file_sync(
            "docs/readme.txt",
            b"This backend stays in memory for wasm32-wasip2 demos.
",
        );
        backend
    }

    fn create_dir_sync(&self, path: &str) {
        let normalized = normalize_path(path).expect("valid directory path");
        let mut state = self.inner.lock().expect("memory backend lock");
        create_dir_locked(&mut state, &normalized).expect("create demo directory");
    }

    fn write_file_sync(&self, path: &str, contents: &[u8]) {
        let normalized = normalize_path(path).expect("valid file path");
        let mut state = self.inner.lock().expect("memory backend lock");
        write_file_locked(&mut state, &normalized, 0, contents.to_vec()).expect("write demo file");
    }
}

#[async_trait]
impl WebDavBackend for MemoryBackend {
    async fn metadata(&self, path: &str) -> Result<Metadata> {
        let normalized = normalize_path(path)?;
        let state = self.inner.lock().expect("memory backend lock");
        let node = state
            .entries
            .get(&normalized)
            .ok_or_else(|| BackendError::NotFound {
                path: path.to_string(),
            })?;
        Ok(node.metadata.clone())
    }

    async fn read_dir(&self, path: &str) -> Result<Vec<DirEntry>> {
        let normalized = normalize_path(path)?;
        let state = self.inner.lock().expect("memory backend lock");
        ensure_dir(&state, &normalized)?;

        let prefix = prefix_for(&normalized);
        let mut entries = Vec::new();
        for (candidate_path, node) in &state.entries {
            if candidate_path.is_empty() {
                continue;
            }
            let Some(rest) = candidate_path.strip_prefix(&prefix) else {
                continue;
            };
            if rest.is_empty() || rest.contains('/') {
                continue;
            }
            entries.push(DirEntry {
                name: rest.to_string(),
                metadata: node.metadata.clone(),
            });
        }
        entries.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(entries)
    }

    async fn read_chunk(&self, path: &str, start: u64, length: u64) -> Result<Vec<u8>> {
        let normalized = normalize_path(path)?;
        let mut state = self.inner.lock().expect("memory backend lock");
        let node = state
            .entries
            .get_mut(&normalized)
            .ok_or_else(|| BackendError::NotFound {
                path: path.to_string(),
            })?;
        let NodeKind::File(bytes) = &mut node.kind else {
            return Err(BackendError::IsDirectory {
                path: path.to_string(),
            });
        };
        node.metadata.accessed = Some(std::time::SystemTime::now());
        let start = usize::try_from(start).map_err(|_| BackendError::FileTooLarge)?;
        let length = usize::try_from(length).map_err(|_| BackendError::FileTooLarge)?;
        if start >= bytes.len() {
            return Ok(Vec::new());
        }
        let end = start.saturating_add(length).min(bytes.len());
        Ok(bytes[start..end].to_vec())
    }

    async fn write_chunk(&self, path: &str, start: u64, bytes: Vec<u8>) -> Result<u64> {
        let normalized = normalize_path(path)?;
        let mut state = self.inner.lock().expect("memory backend lock");
        write_file_locked(&mut state, &normalized, start, bytes)
    }

    async fn create_dir(&self, path: &str) -> Result<()> {
        let normalized = normalize_path(path)?;
        let mut state = self.inner.lock().expect("memory backend lock");
        create_dir_locked(&mut state, &normalized)
    }

    async fn remove_dir(&self, path: &str) -> Result<()> {
        let normalized = normalize_path(path)?;
        if normalized.is_empty() {
            return Err(BackendError::PermissionDenied {
                path: path.to_string(),
            });
        }
        let mut state = self.inner.lock().expect("memory backend lock");
        ensure_dir(&state, &normalized)?;
        let prefix = prefix_for(&normalized);
        if state
            .entries
            .keys()
            .any(|candidate| candidate != &normalized && candidate.starts_with(&prefix))
        {
            return Err(BackendError::DirectoryNotEmpty {
                path: path.to_string(),
            });
        }
        state.entries.remove(&normalized);
        Ok(())
    }

    async fn remove_file(&self, path: &str) -> Result<()> {
        let normalized = normalize_path(path)?;
        let mut state = self.inner.lock().expect("memory backend lock");
        let node = state
            .entries
            .get(&normalized)
            .ok_or_else(|| BackendError::NotFound {
                path: path.to_string(),
            })?;
        if matches!(node.kind, NodeKind::Dir) {
            return Err(BackendError::IsDirectory {
                path: path.to_string(),
            });
        }
        state.entries.remove(&normalized);
        Ok(())
    }

    async fn rename(&self, from: &str, to: &str) -> Result<()> {
        let from = normalize_path(from)?;
        let to = normalize_path(to)?;
        move_entries(&self.inner, &from, &to, false)
    }

    async fn copy(&self, from: &str, to: &str) -> Result<()> {
        let from = normalize_path(from)?;
        let to = normalize_path(to)?;
        move_entries(&self.inner, &from, &to, true)
    }
}

fn ensure_dir(state: &State, path: &str) -> Result<()> {
    let node = state
        .entries
        .get(path)
        .ok_or_else(|| BackendError::NotFound {
            path: path.to_string(),
        })?;
    match node.kind {
        NodeKind::Dir => Ok(()),
        NodeKind::File(_) => Err(BackendError::NotDirectory {
            path: path.to_string(),
        }),
    }
}

fn parent_dir(path: &str) -> Option<&str> {
    if path.is_empty() {
        return None;
    }
    path.rsplit_once('/').map(|(parent, _)| parent).or(Some(""))
}

fn prefix_for(path: &str) -> String {
    if path.is_empty() {
        String::new()
    } else {
        format!("{path}/")
    }
}

fn create_dir_locked(state: &mut State, path: &str) -> Result<()> {
    if path.is_empty() {
        return Ok(());
    }
    if state.entries.contains_key(path) {
        return Err(BackendError::AlreadyExists {
            path: path.to_string(),
        });
    }
    let parent = parent_dir(path).unwrap_or("");
    ensure_dir(state, parent)?;
    state.entries.insert(
        path.to_string(),
        Node {
            metadata: Metadata::directory(),
            kind: NodeKind::Dir,
        },
    );
    Ok(())
}

fn write_file_locked(state: &mut State, path: &str, start: u64, bytes: Vec<u8>) -> Result<u64> {
    if path.is_empty() {
        return Err(BackendError::PermissionDenied {
            path: path.to_string(),
        });
    }
    let parent = parent_dir(path).unwrap_or("");
    ensure_dir(state, parent)?;

    let start = usize::try_from(start).map_err(|_| BackendError::FileTooLarge)?;
    let node = state
        .entries
        .entry(path.to_string())
        .or_insert_with(|| Node {
            metadata: Metadata::file(0),
            kind: NodeKind::File(Vec::new()),
        });
    let NodeKind::File(contents) = &mut node.kind else {
        return Err(BackendError::IsDirectory {
            path: path.to_string(),
        });
    };
    if start > contents.len() {
        contents.resize(start, 0);
    }
    let end = start
        .checked_add(bytes.len())
        .ok_or(BackendError::FileTooLarge)?;
    if end > contents.len() {
        contents.resize(end, 0);
    }
    contents[start..end].copy_from_slice(&bytes);
    node.metadata = Metadata::file(contents.len() as u64);
    Ok(end as u64)
}

fn move_entries(inner: &Arc<Mutex<State>>, from: &str, to: &str, copy_only: bool) -> Result<()> {
    if from.is_empty() || to.is_empty() {
        return Err(BackendError::PermissionDenied {
            path: if from.is_empty() { from } else { to }.to_string(),
        });
    }

    let mut state = inner.lock().expect("memory backend lock");
    if !state.entries.contains_key(from) {
        return Err(BackendError::NotFound {
            path: from.to_string(),
        });
    }
    if state.entries.contains_key(to) {
        return Err(BackendError::AlreadyExists {
            path: to.to_string(),
        });
    }
    let parent = parent_dir(to).unwrap_or("");
    ensure_dir(&state, parent)?;

    let prefix = prefix_for(from);
    let mut moved = Vec::new();
    for (path, node) in &state.entries {
        if path == from || path.starts_with(&prefix) {
            let suffix = path.strip_prefix(from).expect("prefix checked");
            moved.push((format!("{to}{suffix}"), node.clone()));
        }
    }

    if !copy_only {
        let old_keys: Vec<String> = state
            .entries
            .keys()
            .filter(|path| *path == from || path.starts_with(&prefix))
            .cloned()
            .collect();
        for key in old_keys {
            state.entries.remove(&key);
        }
    }

    for (path, node) in moved {
        state.entries.insert(path, node);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "current_thread")]
    async fn writes_and_reads_files() {
        let backend = MemoryBackend::default();
        backend.create_dir("docs").await.unwrap();
        backend
            .write_chunk("docs/test.txt", 0, b"abc".to_vec())
            .await
            .unwrap();
        backend
            .write_chunk("docs/test.txt", 5, b"z".to_vec())
            .await
            .unwrap();

        let data = backend.read_chunk("docs/test.txt", 0, 16).await.unwrap();
        assert_eq!(data, b"abc  z".to_vec());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn copies_directory_trees() {
        let backend = MemoryBackend::default();
        backend.create_dir("docs").await.unwrap();
        backend
            .write_chunk("docs/readme.txt", 0, b"hello".to_vec())
            .await
            .unwrap();

        backend.copy("docs", "backup").await.unwrap();

        let entries = backend.read_dir("backup").await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "readme.txt");
    }
}
