use crate::{
    DEFAULT_CONFIG_FILE, FungiConfig, FungiDir, devices::DevicesConfig,
    direct_addresses::DirectAddressCache, local_access::LocalAccessConfig, paths::FungiPaths,
    service_cache::ServiceCache, trusted_devices::TrustedDevicesConfig,
};
use anyhow::{Context, Result};

pub fn init(dirs: &impl FungiDir, upgrade_existing: bool) -> Result<()> {
    let fungi_dir = dirs.fungi_dir();
    let config_file = fungi_dir.join(DEFAULT_CONFIG_FILE);
    std::fs::create_dir_all(&fungi_dir).ok();

    let migration = crate::migrate_if_needed(&fungi_dir)?;
    if migration.changed {
        println!(
            "Migrated Fungi configuration from {} to v{}.",
            migration.source_version, migration.target_version
        );
        if let Some(backup_dir) = migration.backup_dir {
            println!("Backup saved to {}", backup_dir.display());
        }
    }
    ensure_user_workspace(&fungi_dir)?;

    if config_file.exists() {
        if upgrade_existing {
            let config = FungiConfig::apply_from_dir(&fungi_dir)?;
            config.save_to_file()?;
            DevicesConfig::apply_from_dir(&fungi_dir)?;
            DirectAddressCache::apply_from_dir(&fungi_dir)?;
            LocalAccessConfig::apply_from_dir(&fungi_dir)?;
            ServiceCache::apply_from_dir(&fungi_dir)?;
            TrustedDevicesConfig::apply_from_dir(&fungi_dir)?;
            println!("Configuration file upgraded at {}", config_file.display());
            return Ok(());
        }

        println!(
            "Configuration file already exists at {}",
            config_file.display()
        );
        return Ok(());
    }

    log::info!("Initializing Fungi...");

    // create config.toml
    FungiConfig::apply_from_dir(&fungi_dir)?;

    // create devices.toml
    DevicesConfig::apply_from_dir(&fungi_dir)?;

    // create cache/direct_addresses.json, access/local_access.json, and cache/remote_services/
    DirectAddressCache::apply_from_dir(&fungi_dir)?;
    LocalAccessConfig::apply_from_dir(&fungi_dir)?;
    ServiceCache::apply_from_dir(&fungi_dir)?;
    TrustedDevicesConfig::apply_from_dir(&fungi_dir)?;

    // create .keys
    fungi_util::keypair::init_keypair(&fungi_dir)?;

    log::info!("Fungi initialized at {}", fungi_dir.display());
    Ok(())
}

fn ensure_user_workspace(fungi_dir: &std::path::Path) -> Result<()> {
    let paths = FungiPaths::from_fungi_home(fungi_dir);
    std::fs::create_dir_all(paths.user_home()).with_context(|| {
        format!(
            "Failed to create Fungi user workspace: {}",
            paths.user_home().display()
        )
    })
}
