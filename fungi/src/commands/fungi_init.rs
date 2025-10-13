use anyhow::Result;
use clap::Parser;
use fungi_config::FungiDir;

#[derive(Debug, Clone, Default, Parser)]
pub struct InitArgs {}

pub async fn run(args: impl FungiDir) -> Result<()> {
    fungi_config::init(&args)
}
