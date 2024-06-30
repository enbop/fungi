mod daemon;
mod init;
mod mush;
mod wasi;
pub use daemon::daemon;
pub use init::init;
pub use mush::mush;
pub use wasi::wasi;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

use crate::DEFAULT_FUNGI_DIR;

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

    /// Start a Fungi wasi process
    Wasi,

    Mush,
}

impl FungiArgs {
    pub fn fungi_dir(&self) -> PathBuf {
        self.fungi_dir
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| home::home_dir().unwrap().join(DEFAULT_FUNGI_DIR))
    }
}
