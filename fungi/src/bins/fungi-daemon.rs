use clap::Parser;
use fungi::commands::{self, DaemonArgs};

#[tokio::main]
async fn main() {
    env_logger::init();
    let args = DaemonArgs::parse();
    commands::daemon(args, false).await;
}
