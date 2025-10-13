use anyhow::{Ok, Result};
use clap::Parser;
use fungi::commands::*;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let fungi_args = FungiArgs::parse();

    match fungi_args.command {
        Commands::Daemon(args) => fungi_daemon::run(args).await?,
        Commands::Init(_args) => fungi_init::run(fungi_args.common).await?,
        Commands::Relay(args) => fungi_relay::run(args).await?,
        Commands::Run(args) => fungi_wasi::run(args).await?,
        Commands::Control(cmd) => fungi_control::execute(fungi_args.common, cmd).await,
    }
    Ok(())
}
