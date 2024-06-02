use clap::Parser;
use fungi::commands::{self, Commands, FungiArgs};

#[tokio::main]
async fn main() {
    env_logger::init();
    let args = FungiArgs::parse();
    match args.command {
        Some(Commands::Init) => commands::init(&args),
        Some(Commands::Daemon) => commands::daemon(&args).await,
        None => println!("No command provided"),
    }
}
