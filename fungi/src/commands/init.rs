use std::path::Path;

use crate::DEFAULT_CONFIG_FILE;

use super::FungiArgs;

pub fn init(args: &FungiArgs) {
    println!("Initializing Fungi...");
    let fungi_dir = args.fungi_dir();

    // check if the directory exists
    if fungi_dir.exists() {
        println!("Fungi is already initialized");
        return;
    }
    std::fs::create_dir(&fungi_dir).unwrap();

    // create config.toml
    let config = fungi_dir.join(DEFAULT_CONFIG_FILE);
    std::fs::File::create(config).unwrap();

    // create .keys
    init_keypair(&fungi_dir);

    println!("Fungi initialized at {}", fungi_dir.display());
}

pub fn init_keypair(fungi_dir: &Path) {
    println!("Generating key pair...");
    let keypair = libp2p::identity::Keypair::generate_secp256k1();
    println!(
        "Key pair generated {}:{:?}",
        keypair.key_type(),
        keypair.public()
    );
    let encoded = keypair.to_protobuf_encoding().unwrap();
    std::fs::create_dir(fungi_dir.join(".keys")).unwrap();
    let keypair_file = fungi_dir.join(".keys").join("keypair");
    std::fs::write(&keypair_file, encoded).unwrap();
    println!("Key pair saved at {}", keypair_file.display());
}
