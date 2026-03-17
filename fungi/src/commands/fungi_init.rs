use anyhow::Result;
use clap::Parser;
use fungi_config::FungiDir;

#[derive(Debug, Clone, Default, Parser)]
pub struct InitArgs {
    #[arg(
        long,
        help = "Rewrite config.toml using the current schema and defaults"
    )]
    pub upgrade_config: bool,
}

pub async fn run(common_args: impl FungiDir, args: InitArgs) -> Result<()> {
    fungi_config::init(&common_args, args.upgrade_config)
}
