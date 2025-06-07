use crate::{FungiDir, DEFAULT_CONFIG_FILE};
use std::{io, path::Path};

pub fn init(dirs: &impl FungiDir) -> io::Result<()> {
    let fungi_dir = dirs.fungi_dir();
    // check if the directory exists
    if fungi_dir.exists() {
        return Ok(());
    }
    println!("Initializing Fungi...");
    std::fs::create_dir(&fungi_dir)?;

    // create config.toml
    let config = &fungi_dir.join(DEFAULT_CONFIG_FILE);
    std::fs::File::create(config)?;

    // create .keys
    fungi_util::keypair::init_keypair(&fungi_dir)?;

    println!("Fungi initialized at {}", fungi_dir.display());
    Ok(())
}
