use crate::{DEFAULT_CONFIG_FILE, FungiConfig, FungiDir, address_book::AddressBookConfig};
use anyhow::Result;

pub fn init(dirs: &impl FungiDir) -> Result<()> {
    let fungi_dir = dirs.fungi_dir();
    let config_file = fungi_dir.join(DEFAULT_CONFIG_FILE);
    if config_file.exists() {
        println!(
            "Configuration file already exists at {}",
            config_file.display()
        );
        return Ok(());
    }

    log::info!("Initializing Fungi...");
    std::fs::create_dir(&fungi_dir).ok();

    // create config.toml
    FungiConfig::apply_from_dir(&fungi_dir)?;

    // create address_book.toml
    AddressBookConfig::apply_from_dir(&fungi_dir)?;

    // create .keys
    fungi_util::keypair::init_keypair(&fungi_dir)?;

    log::info!("Fungi initialized at {}", fungi_dir.display());
    Ok(())
}
