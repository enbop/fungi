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

        // control commands
        Commands::Info(cmd) => fungi_runtime(fungi_control::execute_info(fungi_args.common, cmd)),
        Commands::Peer(cmd) => fungi_runtime(fungi_control::execute_peer(fungi_args.common, cmd)),
        Commands::Ft(cmd) => {
            fungi_runtime(fungi_control::execute_file_transfer(fungi_args.common, cmd))
        }
        Commands::Proxy(cmd) => fungi_runtime(fungi_control::execute_proxy(fungi_args.common, cmd)),
        Commands::Tunnel(cmd) => {
            fungi_runtime(fungi_control::execute_tunnel(fungi_args.common, cmd))
        }
        Commands::Device(cmd) => {
            fungi_runtime(fungi_control::execute_device(fungi_args.common, cmd))
        }
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
