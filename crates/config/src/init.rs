use crate::{FungiConfig, FungiDir};
use anyhow::Result;

pub fn init(dirs: &impl FungiDir) -> Result<()> {
    let fungi_dir = dirs.fungi_dir();
    // check if the directory exists
    if fungi_dir.exists() && fungi_dir.is_dir() {
        if fungi_dir.read_dir()?.next().is_some() {
            return Ok(());
        }
    }
    println!("Initializing Fungi...");
    std::fs::create_dir(&fungi_dir)?;

    // create config.toml
    FungiConfig::apply_from_dir(&fungi_dir)?;

    // create .keys
    fungi_util::keypair::init_keypair(&fungi_dir)?;

    println!("Fungi initialized at {}", fungi_dir.display());
    Ok(())
}
