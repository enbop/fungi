use anyhow::{Ok, Result};
use clap::Parser;
use flexi_logger::{Cleanup, Criterion, Duplicate, FileSpec, Logger, Naming};
use fungi::commands::*;
use fungi_config::FungiDir;
use std::sync::Once;

static PANIC_HOOK_INIT: Once = Once::new();

fn main() -> Result<()> {
    let fungi_args = FungiArgs::parse();
    init_logging(&fungi_args)?;
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
        Commands::AllowedPeers(cmd) => block_on(execute_allowed_peer(fungi_args.common, cmd)),
        Commands::FtService(cmd) => block_on(execute_ft_service(fungi_args.common, cmd)),
        Commands::FtClient(cmd) => block_on(execute_ft_client(fungi_args.common, cmd)),
        Commands::Tunnel(cmd) => block_on(execute_tunnel(fungi_args.common, cmd)),
        Commands::Device(cmd) => block_on(execute_device(fungi_args.common, cmd)),
        Commands::Connection(cmd) => block_on(execute_connection(fungi_args.common, cmd)),
        Commands::Ping {
            peer_id,
            interval_ms,
            verbose,
        } => block_on(execute_ping(
            fungi_args.common,
            peer_id,
            interval_ms,
            verbose,
        )),
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

fn init_logging(fungi_args: &FungiArgs) -> Result<()> {
    #[cfg(feature = "wasi")]
    if matches!(&fungi_args.command, Commands::Run(_) | Commands::Serve(_)) {
        return Ok(());
    }

    if matches!(&fungi_args.command, Commands::Daemon(_)) {
        return init_daemon_file_logging(&fungi_args.common);
    }

    let _ = env_logger::try_init();
    Ok(())
}

fn init_daemon_file_logging(common_args: &CommonArgs) -> Result<()> {
    const DEFAULT_LOG_SPEC: &str = "info";
    const MAX_LOG_FILE_SIZE_BYTES: u64 = 5 * 1024 * 1024;
    const MAX_LOG_FILES: usize = 5;

    let log_spec = std::env::var("RUST_LOG")
        .or_else(|_| std::env::var("FUNGI_LOG_LEVEL"))
        .unwrap_or_else(|_| DEFAULT_LOG_SPEC.to_string());

    let log_dir = common_args.fungi_dir().join("logs");
    std::fs::create_dir_all(&log_dir)?;

    let duplicate = if cfg!(debug_assertions) || std::env::var("FUNGI_LOG_STDIO").is_ok() {
        Duplicate::All
    } else {
        Duplicate::Warn
    };

    Logger::try_with_str(log_spec)?
        .log_to_file(
            FileSpec::default()
                .directory(log_dir)
                .basename("daemon")
                .suffix("log"),
        )
        .duplicate_to_stderr(duplicate)
        .rotate(
            Criterion::Size(MAX_LOG_FILE_SIZE_BYTES),
            Naming::Numbers,
            Cleanup::KeepLogFiles(MAX_LOG_FILES),
        )
        .start()?;

    install_panic_hook();

    Ok(())
}

fn install_panic_hook() {
    PANIC_HOOK_INIT.call_once(|| {
        std::panic::set_hook(Box::new(|panic_info| {
            let location = panic_info
                .location()
                .map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()))
                .unwrap_or_else(|| "unknown location".to_string());

            let payload = panic_info
                .payload()
                .downcast_ref::<&str>()
                .map(|s| (*s).to_string())
                .or_else(|| panic_info.payload().downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "non-string panic payload".to_string());

            log::error!("panic at {}: {}", location, payload);
            eprintln!("panic at {}: {}", location, payload);
        }));
    });
}

#[cfg(target_os = "android")]
fn init_device_info(common_args: &CommonArgs) {
    {
        if !common_args.default_device_name.is_empty() {
            fungi_util::init_mobile_device_name(common_args.default_device_name.clone());
        }
    }
}
