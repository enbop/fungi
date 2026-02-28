use crate::{FungiConfig, FungiDir, address_book::AddressBookConfig};
use anyhow::Result;

pub fn init(dirs: &impl FungiDir) -> Result<()> {
    init_impl(dirs, false)
}

pub fn init_for_daemon(dirs: &impl FungiDir) -> Result<()> {
    init_impl(dirs, true)
}

fn init_impl(dirs: &impl FungiDir, daemon_mode: bool) -> Result<()> {
    let fungi_dir = dirs.fungi_dir();
    // check if the directory exists
    if fungi_dir.exists() && fungi_dir.is_dir() && fungi_dir.read_dir()?.next().is_some() {
        if daemon_mode {
            log::info!(
                "Fungi directory already exists and is not empty: {}",
                fungi_dir.display()
            );
        } else {
            println!(
                "Fungi directory already exists and is not empty: {}",
                fungi_dir.display()
            );
        }
        return Ok(());
    }
    if daemon_mode {
        log::info!("Initializing Fungi...");
    } else {
        println!("Initializing Fungi...");
    }
    std::fs::create_dir(&fungi_dir).ok();

    // create config.toml
    FungiConfig::apply_from_dir(&fungi_dir)?;

    // create address_book.toml
    AddressBookConfig::apply_from_dir(&fungi_dir)?;

    // create .keys
    fungi_util::keypair::init_keypair(&fungi_dir)?;

    if daemon_mode {
        log::info!("Fungi initialized at {}", fungi_dir.display());
    } else {
        println!("Fungi initialized at {}", fungi_dir.display());
    }
    Ok(())
}
