use anyhow::{Ok, Result};
use clap::Parser;
use fungi::commands::*;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let fungi_args = FungiArgs::parse();

    match fungi_args.command {
        Commands::Daemon(args) => fungi_daemon::run(args).await?,
        Commands::Init(args) => fungi_init::run(args).await?,
    }
    Ok(())
}
