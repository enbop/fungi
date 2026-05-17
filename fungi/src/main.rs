use anyhow::{Ok, Result};
use clap::Parser;
use fungi::commands::*;
use fungi_config::FungiDir;
mod logging;

fn main() -> Result<()> {
    let fungi_args = FungiArgs::parse();
    run_migration_preflight(&fungi_args)?;
    logging::init_logging(&fungi_args)?;
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
        Commands::Daemon(args) => block_on(fungi_daemon::execute(fungi_args.common.clone(), args))?,
        Commands::Init(args) => block_on(fungi_init::run(fungi_args.common, args))?,
        Commands::Migrate(args) => block_on(fungi_migrate::run(fungi_args.common, args))?,
        Commands::Relay(cmd) => block_on(execute_relay(fungi_args.common, cmd)),

        // control commands
        Commands::Info(cmd) => block_on(execute_info(fungi_args.common, cmd)),
        Commands::Security(cmd) => block_on(execute_security(fungi_args.common, cmd)),
        Commands::Service(cmd) => block_on(execute_service(fungi_args.common, cmd)),
        Commands::Catalog(cmd) => block_on(execute_catalog(fungi_args.common, cmd)),
        Commands::Access(cmd) => block_on(execute_access(fungi_args.common, cmd)),
        Commands::Peer(cmd) => block_on(execute_peer(fungi_args.common, cmd)),
        Commands::Device(cmd) => block_on(execute_device(fungi_args.common, cmd)),
        Commands::Connection(cmd) => block_on(execute_connection(fungi_args.common, cmd)),
        Commands::Ping {
            peer,
            interval_ms,
            verbose,
        } => block_on(execute_ping(fungi_args.common, peer, interval_ms, verbose)),
        Commands::Dynamic(tokens) => {
            let device_context = fungi_args.common.dynamic_device.clone();
            block_on(execute_dynamic_thing(
                fungi_args.common,
                device_context,
                tokens,
            ))
        }
    }

    Ok(())
}

fn run_migration_preflight(fungi_args: &FungiArgs) -> Result<()> {
    #[cfg(feature = "wasi")]
    if matches!(&fungi_args.command, Commands::Run(_) | Commands::Serve(_)) {
        return Ok(());
    }

    if matches!(&fungi_args.command, Commands::Migrate(_)) {
        return Ok(());
    }

    let report = fungi_config::migrate_if_needed(&fungi_args.common.fungi_dir())?;
    if report.changed {
        println!(
            "Migrated Fungi configuration from {} to v{}.",
            report.source_version, report.target_version
        );
        if let Some(backup_dir) = report.backup_dir {
            println!("Backup saved to {}", backup_dir.display());
        }
    }
    Ok(())
}

fn block_on<F: Future>(f: F) -> F::Output {
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
