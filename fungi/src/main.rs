use clap::{Parser, Subcommand};
use fungi::commands;

/// Fungi the world!
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Initialize Fungi
    Init,

    /// Start a Fungi daemon
    Daemon,
}

fn main() {
    env_logger::init();
    let args = Args::parse();
    match args.command {
        Some(Commands::Init) => commands::init(),
        Some(Commands::Daemon) => commands::daemon(),
        None => println!("No command provided"),
    }
}
