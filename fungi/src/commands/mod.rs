mod init;
use std::path::PathBuf;

pub use init::init;
mod daemon;
use clap::{Parser, Subcommand};
pub use daemon::daemon;

const FUNGI_DIR: &'static str = ".fungi";

/// Fungi the world!
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct FungiArgs {
    #[command(subcommand)]
    pub command: Option<Commands>,

    #[arg(short, long)]
    pub fungi_dir: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Initialize Fungi
    Init,

    /// Start a Fungi daemon
    Daemon,
}

impl FungiArgs {
    pub fn fungi_dir(&self) -> PathBuf {
        self.fungi_dir
            .as_ref()
            .map(|p| PathBuf::from(p))
            .unwrap_or_else(|| home::home_dir().unwrap().join(FUNGI_DIR))
    }
}
