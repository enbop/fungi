use anyhow::{Ok, Result};
use clap::Parser;
use fungi::commands::*;

fn main() -> Result<()> {
    let fungi_args = FungiArgs::parse();
    use fungi_control::*;

    #[cfg(target_os = "android")]
    init_device_info(&fungi_args.common);

    match fungi_args.command {
        #[cfg(feature = "wasi")]
        // wasmtime commands
        // env_logger and tokio runtime have been initialized in wasmtime commands
        Commands::Run(c) => c.execute()?,
        #[cfg(feature = "wasi")]
        Commands::Serve(c) => c.execute()?,

        // fungi commands
        Commands::Daemon(args) => block_on(fungi_daemon::run(args))?,
        Commands::Init(_args) => block_on(fungi_init::run(fungi_args.common))?,
        Commands::Relay(args) => block_on(fungi_relay::run(args))?,

        // control commands
        Commands::Info(cmd) => block_on(execute_info(fungi_args.common, cmd)),
        Commands::AllowedPeer(cmd) => block_on(execute_allowed_peer(fungi_args.common, cmd)),
        Commands::FtService(cmd) => block_on(execute_ft_service(fungi_args.common, cmd)),
        Commands::FtClient(cmd) => block_on(execute_ft_client(fungi_args.common, cmd)),
        Commands::Tunnel(cmd) => block_on(execute_tunnel(fungi_args.common, cmd)),
        Commands::Device(cmd) => block_on(execute_device(fungi_args.common, cmd)),
    }

    Ok(())
}

fn block_on<F: Future>(f: F) -> F::Output {
    env_logger::init();

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(f)
}

#[cfg(target_os = "android")]
fn init_device_info(common_args: &CommonArgs) {
    {
        if !common_args.default_device_name.is_empty() {
            fungi_util::init_mobile_device_name(common_args.default_device_name.clone());
        }
    }
}
