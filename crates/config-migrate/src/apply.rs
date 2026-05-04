use anyhow::{Context, Result, bail};
use chrono::Utc;
use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{
    detection::detect_source_version,
    model::{
        APPLY_ROLLBACK_DIR_NAME, BACKUP_ROOT_DIR, CURRENT_FUNGI_DIR_VERSION, DEVICES_FILE,
        DetectedVersion, LEGACY_ADDRESS_BOOK_FILE, LEGACY_SERVICE_STATE_FILE, MigrationPlan,
        STAGING_DIR_PREFIX,
    },
};

#[derive(Debug)]
pub(crate) struct MigrationTransaction {
    pub(crate) backup_dir: PathBuf,
    pub(crate) staging_dir: PathBuf,
}

#[derive(Debug)]
struct AppliedPath {
    relative_path: PathBuf,
    moved_live_to_rollback: bool,
    moved_staged_to_live: bool,
}

impl MigrationTransaction {
    pub(crate) fn prepare(
        fungi_dir: &Path,
        source_version: &DetectedVersion,
        target_version: u32,
    ) -> Result<Self> {
        let timestamp = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        let backup_root = fungi_dir.join(BACKUP_ROOT_DIR);
        fs::create_dir_all(&backup_root).with_context(|| {
            format!(
                "Failed to create migration backup root directory: {}",
                backup_root.display()
            )
        })?;

        let backup_dir = backup_root.join(format!(
            "{timestamp}-from-{}-to-v{target_version}",
            source_version.backup_label()
        ));
        let staging_dir =
            fungi_dir.join(format!("{STAGING_DIR_PREFIX}{timestamp}-v{target_version}"));

        if backup_dir.exists() {
            bail!(
                "Refusing to overwrite existing migration backup directory: {}",
                backup_dir.display()
            );
        }
        if staging_dir.exists() {
            bail!(
                "Refusing to overwrite existing migration staging directory: {}",
                staging_dir.display()
            );
        }

        fs::create_dir_all(&backup_dir).with_context(|| {
            format!(
                "Failed to create migration backup directory: {}",
                backup_dir.display()
            )
        })?;
        fs::create_dir_all(&staging_dir).with_context(|| {
            format!(
                "Failed to create migration staging directory: {}",
                staging_dir.display()
            )
        })?;

        Ok(Self {
            backup_dir,
            staging_dir,
        })
    }
}

pub(crate) fn copy_selected_paths(
    source_root: &Path,
    target_root: &Path,
    relative_paths: &[PathBuf],
) -> Result<()> {
    for relative_path in relative_paths {
        if relative_path.is_absolute() {
            bail!(
                "Migration path must be relative to the fungi-dir root: {}",
                relative_path.display()
            );
        }

        let source = source_root.join(relative_path);
        if !source.exists() {
            continue;
        }

        copy_path_recursively(&source, &target_root.join(relative_path))?;
    }
    Ok(())
}

pub(crate) fn validate_migrated_dir(staging_root: &Path, plan: &MigrationPlan) -> Result<()> {
    if plan.update_config {
        match detect_source_version(staging_root)? {
            DetectedVersion::Version(version) if version == CURRENT_FUNGI_DIR_VERSION => {}
            other => {
                bail!(
                    "Migration validation failed; expected v{} but found {}",
                    CURRENT_FUNGI_DIR_VERSION,
                    other
                )
            }
        }
    }

    if plan.migrate_address_book {
        if staging_root.join(LEGACY_ADDRESS_BOOK_FILE).exists() {
            bail!("Migration validation failed; legacy address_book.toml still exists in staging");
        }
        if !staging_root.join(DEVICES_FILE).exists() {
            bail!("Migration validation failed; devices.toml was not created in staging");
        }
    }

    if plan.migrate_legacy_managed_services && staging_root.join(LEGACY_SERVICE_STATE_FILE).exists()
    {
        bail!("Migration validation failed; legacy services-state.json still exists in staging");
    }

    Ok(())
}

pub(crate) fn apply_staged_paths(
    staging_root: &Path,
    live_root: &Path,
    migrated_paths: &[PathBuf],
) -> Result<()> {
    let rollback_root = staging_root.join(APPLY_ROLLBACK_DIR_NAME);
    fs::create_dir_all(&rollback_root).with_context(|| {
        format!(
            "Failed to create migration apply rollback directory in staging: {}",
            rollback_root.display()
        )
    })?;

    let mut applied_paths = Vec::new();
    for relative_path in migrated_paths {
        match apply_one_staged_path(staging_root, live_root, &rollback_root, relative_path) {
            Ok(applied_path) => applied_paths.push(applied_path),
            Err(error) => {
                if let Err(rollback_error) =
                    rollback_applied_paths(live_root, &rollback_root, &applied_paths)
                {
                    return Err(error).context(format!(
                        "Additionally failed to roll back migration apply: {rollback_error}"
                    ));
                }
                return Err(error);
            }
        }
    }

    Ok(())
}

fn copy_path_recursively(source: &Path, target: &Path) -> Result<()> {
    let file_type = fs::symlink_metadata(source)
        .with_context(|| {
            format!(
                "Failed to read file metadata during migration: {}",
                source.display()
            )
        })?
        .file_type();

    if file_type.is_symlink() {
        bail!(
            "Symlinked paths are not supported by the migration tool yet: {}",
            source.display()
        );
    }

    if file_type.is_dir() {
        fs::create_dir_all(target).with_context(|| {
            format!(
                "Failed to create target directory during migration copy: {}",
                target.display()
            )
        })?;
        for entry in fs::read_dir(source).with_context(|| {
            format!(
                "Failed to read source directory during migration copy: {}",
                source.display()
            )
        })? {
            let entry = entry?;
            let target_path = target.join(entry.file_name());
            copy_path_recursively(&entry.path(), &target_path)?;
        }
        return Ok(());
    }

    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "Failed to create parent directory during migration copy: {}",
                parent.display()
            )
        })?;
    }
    fs::copy(source, target).with_context(|| {
        format!(
            "Failed to copy file during migration from {} to {}",
            source.display(),
            target.display()
        )
    })?;
    Ok(())
}

fn apply_one_staged_path(
    staging_root: &Path,
    live_root: &Path,
    rollback_root: &Path,
    relative_path: &Path,
) -> Result<AppliedPath> {
    let staged_path = staging_root.join(relative_path);
    let live_path = live_root.join(relative_path);
    let rollback_path = rollback_root.join(relative_path);

    let live_exists = live_path.exists();
    let staged_exists = staged_path.exists();
    let mut moved_live_to_rollback = false;

    if live_exists {
        ensure_parent_dir(&rollback_path)?;
        fs::rename(&live_path, &rollback_path).with_context(|| {
            format!(
                "Failed to move live path aside during migration finalize: {}",
                live_path.display()
            )
        })?;
        moved_live_to_rollback = true;
    }

    if staged_exists {
        ensure_parent_dir(&live_path)?;
        if let Err(error) = fs::rename(&staged_path, &live_path) {
            if moved_live_to_rollback {
                let _ = fs::rename(&rollback_path, &live_path);
            }
            return Err(error).with_context(|| {
                format!(
                    "Failed to move staged path into place during migration finalize: {}",
                    live_path.display()
                )
            });
        }
    }

    Ok(AppliedPath {
        relative_path: relative_path.to_path_buf(),
        moved_live_to_rollback,
        moved_staged_to_live: staged_exists,
    })
}

fn rollback_applied_paths(
    live_root: &Path,
    rollback_root: &Path,
    applied_paths: &[AppliedPath],
) -> Result<()> {
    for applied_path in applied_paths.iter().rev() {
        let live_path = live_root.join(&applied_path.relative_path);
        let rollback_path = rollback_root.join(&applied_path.relative_path);

        if applied_path.moved_staged_to_live {
            remove_path_if_exists(&live_path)?;
        }
        if applied_path.moved_live_to_rollback {
            ensure_parent_dir(&live_path)?;
            fs::rename(&rollback_path, &live_path).with_context(|| {
                format!(
                    "Failed to restore live path during migration rollback: {}",
                    live_path.display()
                )
            })?;
        }
    }
    Ok(())
}

fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "Failed to create parent directory during migration finalize: {}",
                parent.display()
            )
        })?;
    }
    Ok(())
}

fn remove_path_if_exists(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }

    if path.is_dir() {
        fs::remove_dir_all(path).with_context(|| {
            format!(
                "Failed to remove directory while rolling back migration finalize: {}",
                path.display()
            )
        })?;
    } else {
        fs::remove_file(path).with_context(|| {
            format!(
                "Failed to remove file while rolling back migration finalize: {}",
                path.display()
            )
        })?;
    }
    Ok(())
}
