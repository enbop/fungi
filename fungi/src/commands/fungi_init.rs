use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use fungi_config::{DEFAULT_FUNGI_DIR, FungiDir};

#[derive(Debug, Clone, Default, Parser)]
pub struct InitArgs {
    #[clap(short, long, help = "Path to the Fungi directory")]
    pub fungi_dir: Option<String>,
}

impl InitArgs {}

impl FungiDir for InitArgs {
    fn fungi_dir(&self) -> PathBuf {
        self.fungi_dir
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| home::home_dir().unwrap().join(DEFAULT_FUNGI_DIR))
    }
}

pub async fn run(args: InitArgs) -> Result<()> {
    fungi_config::init(&args)
}
