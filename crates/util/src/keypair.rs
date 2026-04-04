use libp2p_identity::Keypair;
use std::path::Path;

const KEY_DIR_NAME: &str = ".keys";
const KEYPAIR_FILE_NAME: &str = "keypair";

pub fn init_keypair(fungi_dir: &Path) -> std::io::Result<Keypair> {
    println!("Generating key pair...");
    let keypair = libp2p_identity::Keypair::generate_secp256k1();
    println!(
        "Key pair generated {}:{:?}",
        keypair.key_type(),
        keypair.public()
    );
    let encoded = keypair.to_protobuf_encoding().unwrap();
    std::fs::create_dir(fungi_dir.join(KEY_DIR_NAME))?;
    let keypair_file = fungi_dir.join(KEY_DIR_NAME).join(KEYPAIR_FILE_NAME);
    std::fs::write(&keypair_file, encoded)?;
    println!("Key pair saved at {}", keypair_file.display());
    Ok(keypair)
}

pub fn get_keypair_from_dir(fungi_dir: &Path) -> anyhow::Result<Keypair> {
    let keypair_file = fungi_dir.join(KEY_DIR_NAME).join(KEYPAIR_FILE_NAME);
    let encoded = std::fs::read(keypair_file)?;
    let keypair = Keypair::from_protobuf_encoding(&encoded)?;
    Ok(keypair)
}
