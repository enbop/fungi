use clap::Parser;
use fungi::commands::{self, WasiArgs};

#[tokio::main]
async fn main() {
    env_logger::init();
    let args = WasiArgs::parse();
    commands::wasi(args).await;
}
