use clap::Parser;
use fungi::commands::*;

#[tokio::main]
async fn main() {
    env_logger::init();
    let fungi_args = FungiArgs::parse();

    if let Some(sub_commands) = fungi_args.command {
        match sub_commands {
            #[cfg(feature = "daemon")]
            Commands::Daemon(args) => fungi_daemon::run(args).await,
        }
    } else {
        fungi_main::run(fungi_args).await
    }
}
