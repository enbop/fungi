use clap::Parser;
use fungi::{
    commands::{self, Commands, FungiArgs},
    config::FungiConfig,
};

#[tokio::main]
async fn main() {
    env_logger::init();
    let args = FungiArgs::parse();
    let config = FungiConfig::parse_from_dir(&args.fungi_dir()).unwrap();
    match args.command {
        Some(Commands::Init) => commands::init(&args),
        Some(Commands::Daemon) => commands::daemon(&args, &config).await,
        None => println!("No command provided"),
    }
}
