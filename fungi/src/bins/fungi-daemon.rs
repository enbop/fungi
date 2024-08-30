#[cfg(feature = "daemon")]
#[tokio::main]
async fn main() {
    use clap::Parser;
    use fungi::commands;
    use fungi_daemon::DaemonArgs;

    env_logger::init();
    let args = DaemonArgs::parse();
    commands::fungi_daemon::run(args).await;
}

#[cfg(not(feature = "daemon"))]
fn main() {
    unreachable!("This binary only works with the daemon feature enabled");
}