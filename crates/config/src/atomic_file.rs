use anyhow::{Context as _, Result};
#[cfg(unix)]
use std::fs::File;
use std::{io::Write, path::Path};
use tempfile::NamedTempFile;

pub(crate) fn write_atomically(path: &Path, contents: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .with_context(|| format!("cache path has no parent directory: {}", path.display()))?;
    std::fs::create_dir_all(parent)
        .with_context(|| format!("failed to create cache directory: {}", parent.display()))?;

    let mut temp_file = NamedTempFile::new_in(parent).with_context(|| {
        format!(
            "failed to create cache temp file in directory: {}",
            parent.display()
        )
    })?;

    if let Ok(metadata) = path.metadata() {
        temp_file
            .as_file_mut()
            .set_permissions(metadata.permissions())
            .with_context(|| {
                format!(
                    "failed to copy permissions to cache temp file: {}",
                    temp_file.path().display()
                )
            })?;
    }

    temp_file.write_all(contents).with_context(|| {
        format!(
            "failed to write cache temp file: {}",
            temp_file.path().display()
        )
    })?;
    temp_file.flush().with_context(|| {
        format!(
            "failed to flush cache temp file: {}",
            temp_file.path().display()
        )
    })?;
    temp_file.as_file().sync_all().with_context(|| {
        format!(
            "failed to sync cache temp file: {}",
            temp_file.path().display()
        )
    })?;

    temp_file.persist(path).map_err(|error| {
        anyhow::anyhow!(
            "failed to replace cache file {} with temp file {}: {}",
            path.display(),
            error.file.path().display(),
            error.error
        )
    })?;
    sync_parent_dir(parent)?;
    Ok(())
}

#[cfg(unix)]
fn sync_parent_dir(parent: &Path) -> Result<()> {
    File::open(parent)
        .with_context(|| format!("failed to open cache directory: {}", parent.display()))?
        .sync_all()
        .with_context(|| format!("failed to sync cache directory: {}", parent.display()))
}

#[cfg(not(unix))]
fn sync_parent_dir(_parent: &Path) -> Result<()> {
    Ok(())
}
