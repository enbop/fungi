use clap::Parser;
use fungi::commands::{self, MushArgs};

#[tokio::main]
async fn main() {
    env_logger::init();
    let args = MushArgs::parse();
    commands::mush(args).await;
}
