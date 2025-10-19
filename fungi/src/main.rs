use anyhow::{Ok, Result};
use clap::Parser;
use fungi::commands::*;

fn main() -> Result<()> {
    let fungi_args = FungiArgs::parse();

    match fungi_args.command {
        // wasmtime commands
        // env_logger and tokio runtime have been initialized in wasmtime commands
        Commands::Run(c) => c.execute()?,
        Commands::Serve(c) => c.execute()?,

        // fungi commands
        Commands::Daemon(args) => fungi_runtime(fungi_daemon::run(args))?,
        Commands::Init(_args) => fungi_runtime(fungi_init::run(fungi_args.common))?,
        Commands::Relay(args) => fungi_runtime(fungi_relay::run(args))?,
        Commands::Control(cmd) => fungi_runtime(fungi_control::execute(fungi_args.common, cmd)),
    }

    Ok(())
}

fn fungi_runtime<F: Future>(f: F) -> F::Output {
    env_logger::init();

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(f)
}
