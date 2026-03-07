use crate::{DEFAULT_CONFIG_FILE, FungiConfig, FungiDir, address_book::AddressBookConfig};
use anyhow::Result;

pub fn init(dirs: &impl FungiDir, upgrade_existing: bool) -> Result<()> {
    let fungi_dir = dirs.fungi_dir();
    let config_file = fungi_dir.join(DEFAULT_CONFIG_FILE);
    std::fs::create_dir_all(&fungi_dir).ok();

    if config_file.exists() {
        if upgrade_existing {
            let config = FungiConfig::apply_from_dir(&fungi_dir)?;
            config.save_to_file()?;
            AddressBookConfig::apply_from_dir(&fungi_dir)?;
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

    // create address_book.toml
    AddressBookConfig::apply_from_dir(&fungi_dir)?;

    // create .keys
    fungi_util::keypair::init_keypair(&fungi_dir)?;

    log::info!("Fungi initialized at {}", fungi_dir.display());
    Ok(())
}
