pub mod fungi_daemon;
pub mod fungi_init;
use clap::{Parser, Subcommand};

/// A platform built for seamless multi-device integration
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
    Init(fungi_init::InitArgs),
}

// impl FungiDir for FungiArgs {
//     fn fungi_dir(&self) -> PathBuf {
//         self.fungi_dir
//             .as_ref()
//             .map(PathBuf::from)
//             .unwrap_or_else(|| home::home_dir().unwrap().join(DEFAULT_FUNGI_DIR))
//     }
// }
