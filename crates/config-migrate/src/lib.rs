mod apply;
mod config_files;
mod detection;
mod model;
mod services;

#[cfg(test)]
mod tests;

use anyhow::{Result, bail};
use std::path::Path;

use apply::{MigrationTransaction, apply_staged_paths, copy_selected_paths, validate_migrated_dir};
use config_files::{
    migrate_config_toml_to_current, migrate_legacy_address_book,
    migrate_legacy_incoming_allowed_peers,
};
use detection::build_migration_plan;
pub use detection::detect_source_version;
pub use model::{CURRENT_FUNGI_DIR_VERSION, DetectedVersion, MigrationReport};
use services::migrate_legacy_services_state;

pub fn migrate_if_needed(fungi_dir: &Path) -> Result<MigrationReport> {
    let source_version = detect_source_version(fungi_dir)?;
    let plan = build_migration_plan(fungi_dir, &source_version)?;

    match source_version.clone() {
        DetectedVersion::MissingConfig if plan.is_empty() => {
            Ok(MigrationReport::no_changes(source_version))
        }
        DetectedVersion::MissingConfig => bail!(
            "Found legacy Fungi side files without config.toml in {}. Initialize or restore the fungi-dir before migrating.",
            fungi_dir.display()
        ),
        DetectedVersion::Version(version)
            if version == CURRENT_FUNGI_DIR_VERSION && plan.is_empty() =>
        {
            Ok(MigrationReport::no_changes(source_version))
        }
        DetectedVersion::LegacyNoVersion | DetectedVersion::Version(CURRENT_FUNGI_DIR_VERSION) => {
            migrate_with_plan(fungi_dir, source_version, plan)
        }
        DetectedVersion::Version(version) => bail!(
            "Unsupported fungi-dir version {version}; no migration path to v{} is implemented yet",
            CURRENT_FUNGI_DIR_VERSION
        ),
    }
}

fn migrate_with_plan(
    fungi_dir: &Path,
    source_version: DetectedVersion,
    plan: model::MigrationPlan,
) -> Result<MigrationReport> {
    if plan.is_empty() {
        return Ok(MigrationReport::no_changes(source_version));
    }

    let transaction =
        MigrationTransaction::prepare(fungi_dir, &source_version, CURRENT_FUNGI_DIR_VERSION)?;
    copy_selected_paths(fungi_dir, &transaction.backup_dir, &plan.touched_paths)?;
    copy_selected_paths(fungi_dir, &transaction.staging_dir, &plan.touched_paths)?;

    if plan.migrate_incoming_allowed_peers {
        migrate_legacy_incoming_allowed_peers(&transaction.staging_dir)?;
    }
    if plan.update_config {
        migrate_config_toml_to_current(&transaction.staging_dir, fungi_dir)?;
    }
    if plan.migrate_address_book {
        migrate_legacy_address_book(&transaction.staging_dir)?;
    }
    if plan.migrate_legacy_managed_services {
        migrate_legacy_services_state(&transaction.staging_dir, fungi_dir)?;
    }

    validate_migrated_dir(&transaction.staging_dir, &plan)?;
    apply_staged_paths(&transaction.staging_dir, fungi_dir, &plan.touched_paths)?;
    std::fs::remove_dir_all(&transaction.staging_dir)?;

    Ok(MigrationReport {
        source_version,
        target_version: CURRENT_FUNGI_DIR_VERSION,
        backup_dir: Some(transaction.backup_dir),
        staging_dir: None,
        migrated_paths: plan.touched_paths,
        changed: true,
    })
}
