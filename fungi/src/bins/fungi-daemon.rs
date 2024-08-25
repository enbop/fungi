use clap::Parser;
use fungi::commands;
use fungi_daemon::DaemonArgs;

#[tokio::main]
async fn main() {
    env_logger::init();
    let args = DaemonArgs::parse();
    commands::daemon(args, false).await;
}
