use anyhow::Result;
use clap::Parser;
use fungi_config::FungiDir;

#[derive(Debug, Clone, Default, Parser)]
pub struct MigrateArgs {}

pub async fn run(common_args: impl FungiDir, _args: MigrateArgs) -> Result<()> {
    let fungi_dir = common_args.fungi_dir();
    let report = fungi_config::migrate_if_needed(&fungi_dir)?;

    if report.changed {
        println!(
            "Migrated Fungi configuration from {} to v{}.",
            report.source_version, report.target_version
        );
        if let Some(backup_dir) = report.backup_dir {
            println!("Backup saved to {}", backup_dir.display());
        }
        if let Some(staging_dir) = report.staging_dir {
            println!("Staging directory retained at {}", staging_dir.display());
        }
        return Ok(());
    }

    match report.source_version {
        fungi_config::FungiDirDetectedVersion::MissingConfig => {
            println!("No existing Fungi configuration found. Nothing to migrate.")
        }
        fungi_config::FungiDirDetectedVersion::Version(version)
            if version == report.target_version =>
        {
            println!("Fungi configuration is already at version {version}.")
        }
        _ => println!("No migration needed."),
    }

    Ok(())
}
