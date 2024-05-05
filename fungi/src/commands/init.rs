const FUNGI_DIR: &'static str = ".fungi";

pub fn init() {
    println!("Initializing Fungi...");
    let home = home::home_dir().unwrap();
    log::debug!("Home directory: {}", home.display());
    let fungi_dir = home.join(FUNGI_DIR);
    // check if the directory exists
    if fungi_dir.exists() {
        println!("Fungi is already initialized");
        return;
    }
    std::fs::create_dir(&fungi_dir).unwrap();

    // create config.toml
    let config = fungi_dir.join("config.toml");
    std::fs::File::create(&config).unwrap();

    println!("Fungi initialized at {}", fungi_dir.display());
}
