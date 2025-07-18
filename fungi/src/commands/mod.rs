pub mod fungi_daemon;
use clap::{Parser, Subcommand};

/// Fungi the world!
#[derive(Parser, Debug, Clone)]
#[command(version, about, long_about = None)]
pub struct FungiArgs {
    #[command(subcommand)]
    pub command: Commands,
    // #[arg(short, long)]
    // pub fungi_dir: Option<String>,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Start a Fungi daemon
    Daemon(fungi_daemon::DaemonArgs),
}

// impl FungiDir for FungiArgs {
//     fn fungi_dir(&self) -> PathBuf {
//         self.fungi_dir
//             .as_ref()
//             .map(PathBuf::from)
//             .unwrap_or_else(|| home::home_dir().unwrap().join(DEFAULT_FUNGI_DIR))
//     }
// }
