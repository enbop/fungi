use anyhow::Result;
use clap::Parser;

fn main() -> Result<()> {
    fungi_lab::LabCli::parse().run()
}
