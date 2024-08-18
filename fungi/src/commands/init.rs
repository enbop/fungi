use crate::DEFAULT_CONFIG_FILE;
use std::{io, path::Path};

use super::FungiDir;

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
    init_keypair(&fungi_dir)?;

    // create wasi root and bin
    let wasi_root = dirs.wasi_root_dir();
    let wasi_bin = dirs.wasi_bin_dir();
    std::fs::create_dir(&wasi_root)?;
    std::fs::create_dir(&wasi_bin)?;

    println!("Fungi initialized at {}", fungi_dir.display());
    Ok(())
}

pub fn init_keypair(fungi_dir: &Path) -> io::Result<()> {
    println!("Generating key pair...");
    let keypair = libp2p::identity::Keypair::generate_secp256k1();
    println!(
        "Key pair generated {}:{:?}",
        keypair.key_type(),
        keypair.public()
    );
    let encoded = keypair.to_protobuf_encoding().unwrap();
    std::fs::create_dir(fungi_dir.join(".keys"))?;
    let keypair_file = fungi_dir.join(".keys").join("keypair");
    std::fs::write(&keypair_file, encoded)?;
    println!("Key pair saved at {}", keypair_file.display());
    Ok(())
}
