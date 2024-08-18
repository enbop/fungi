use clap::Parser;
use fungi::commands::{self, Commands, FungiArgs};

#[tokio::main]
async fn main() {
    env_logger::init();
    let fungi_args = FungiArgs::parse();

    match fungi_args.command {
        Some(Commands::Daemon(args)) => commands::daemon(args, true).await,
        Some(Commands::Wasi(args)) => commands::wasi(args).await,
        Some(Commands::Mush(args)) => commands::mush(args).await,
        None => println!("No command provided"),
    }
}
