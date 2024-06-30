use clap::Parser;
use fungi::{
    commands::{self, Commands, FungiArgs},
    config::FungiConfig,
};

#[tokio::main]
async fn main() {
    env_logger::init();
    let args = FungiArgs::parse();
    let config = match args.command {
        Some(Commands::Init) => FungiConfig::default(),
        _ => FungiConfig::parse_from_dir(&args.fungi_dir()).unwrap(),
    };

    match args.command {
        Some(Commands::Init) => commands::init(&args),
        Some(Commands::Daemon) => commands::daemon(&args, &config).await,
        Some(Commands::Wasi) => commands::wasi(&args).await,
        Some(Commands::Mush) => commands::mush().await,
        None => println!("No command provided"),
    }
}
