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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn init_keypair_creates_keys_directory_and_file() {
        let dir = TempDir::new().unwrap();
        init_keypair(dir.path()).unwrap();
        assert!(dir.path().join(".keys").exists());
        assert!(dir.path().join(".keys").join("keypair").exists());
    }

    #[test]
    fn init_keypair_returns_secp256k1_keypair() {
        let dir = TempDir::new().unwrap();
        let kp = init_keypair(dir.path()).unwrap();
        assert_eq!(kp.key_type(), libp2p_identity::KeyType::Secp256k1);
    }

    #[test]
    fn get_keypair_from_dir_reads_written_keypair() {
        let dir = TempDir::new().unwrap();
        let original = init_keypair(dir.path()).unwrap();
        let loaded = get_keypair_from_dir(dir.path()).unwrap();
        assert_eq!(original.public(), loaded.public());
    }

    #[test]
    fn get_keypair_from_dir_missing_file_returns_error() {
        let dir = TempDir::new().unwrap();
        let result = get_keypair_from_dir(dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn init_keypair_errors_if_keys_dir_already_exists() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".keys")).unwrap();
        let result = init_keypair(dir.path());
        assert!(result.is_err());
    }
}
