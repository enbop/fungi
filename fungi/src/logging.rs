use anyhow::{Ok, Result};
use flexi_logger::{Cleanup, Criterion, Duplicate, FileSpec, Logger, Naming};
use fungi::commands::{Commands, FungiArgs};
use fungi_config::FungiDir;
use std::path::PathBuf;
use std::sync::Once;

static PANIC_HOOK_INIT: Once = Once::new();

pub fn init_logging(fungi_args: &FungiArgs) -> Result<()> {
    #[cfg(feature = "wasi")]
    if matches!(&fungi_args.command, Commands::Run(_) | Commands::Serve(_)) {
        return Ok(());
    }

    if let Commands::Daemon(_) = &fungi_args.command {
        return init_daemon_file_logging(fungi_args.common.fungi_dir());
    }

    let _ = env_logger::try_init();
    Ok(())
}

fn init_daemon_file_logging(fungi_dir: PathBuf) -> Result<()> {
    const DEFAULT_LOG_SPEC: &str = "info";
    const MAX_LOG_FILE_SIZE_BYTES: u64 = 5 * 1024 * 1024;
    const MAX_LOG_FILES: usize = 5;

    let log_spec = std::env::var("RUST_LOG")
        .or_else(|_| std::env::var("FUNGI_LOG_LEVEL"))
        .unwrap_or_else(|_| DEFAULT_LOG_SPEC.to_string());

    let log_dir = fungi_dir.join("logs");
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
