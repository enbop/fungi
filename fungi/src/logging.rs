use anyhow::{Ok, Result};
use env_logger::Env;
use flexi_logger::{Cleanup, Criterion, DeferredNow, Duplicate, FileSpec, Logger, Naming};
use fungi::commands::{Commands, FungiArgs};
use fungi_config::FungiDir;
use log::Record;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Once;

static PANIC_HOOK_INIT: Once = Once::new();
const DEFAULT_LOG_SPEC: &str = "info,libp2p_mdns::behaviour=warn";

pub fn init_logging(fungi_args: &FungiArgs) -> Result<()> {
    #[cfg(feature = "wasi")]
    if matches!(&fungi_args.command, Commands::Run(_) | Commands::Serve(_)) {
        return Ok(());
    }

    if let Commands::Daemon(_) = &fungi_args.command {
        return init_daemon_file_logging(fungi_args.common.fungi_dir());
    }

    let _ = env_logger::Builder::from_env(Env::default().default_filter_or(DEFAULT_LOG_SPEC))
        .format_timestamp_millis()
        .try_init();
    Ok(())
}

fn init_daemon_file_logging(fungi_dir: PathBuf) -> Result<()> {
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
        .format_for_files(daemon_log_format)
        .format_for_stderr(daemon_log_format)
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

fn daemon_log_format(
    write: &mut dyn Write,
    now: &mut DeferredNow,
    record: &Record<'_>,
) -> std::io::Result<()> {
    write!(
        write,
        "{} {:<5} [{}] {}\n",
        now.now().format("%Y-%m-%d %H:%M:%S%.3f%:z"),
        record.level(),
        record.module_path().unwrap_or(record.target()),
        record.args()
    )
}

fn install_panic_hook() {
    PANIC_HOOK_INIT.call_once(|| {
        std::panic::set_hook(Box::new(|panic_info| {
            let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f%:z");
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
            eprintln!(
                "{} ERROR [panic] panic at {}: {}",
                timestamp, location, payload
            );
        }));
    });
}
