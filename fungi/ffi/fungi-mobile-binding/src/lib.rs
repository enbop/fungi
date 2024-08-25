uniffi::include_scaffolding!("export");

use std::sync::Mutex;

use fungi::{self, commands::{daemon::{FungiDaemon, ALL_IN_ONE_BINARY}, init, DaemonArgs}};
use once_cell::sync::Lazy;
use tokio::{runtime::Runtime, sync::oneshot};
use uniffi::deps::log::{self, LevelFilter};

static TOKIO_RUNTIME: Lazy<Runtime> = Lazy::new(|| Runtime::new().unwrap());
static FUNGI_DAEMON_CANCEL_TX: Lazy<Mutex<Option<oneshot::Sender<()>>>> = Lazy::new(|| Default::default());

enum LogLevel {
    Off,
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl Into<LevelFilter> for LogLevel {
    fn into(self) -> LevelFilter {
        match self {
            LogLevel::Off => LevelFilter::Off,
            LogLevel::Error => LevelFilter::Error,
            LogLevel::Warn => LevelFilter::Warn,
            LogLevel::Info => LevelFilter::Info,
            LogLevel::Debug => LevelFilter::Debug,
            LogLevel::Trace => LevelFilter::Trace,
        }
    }
}

#[allow(unused_variables)]
fn init_logger(level: LogLevel) {
    #[cfg(target_os = "android")]
    android_logger::init_once(android_logger::Config::default().with_max_level(level.into()));
}

fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn start_fungi_daemon_block(fungi_dir: String, wasi_bin_path: String) {
    ALL_IN_ONE_BINARY.set(false).unwrap();
    let (tx, rx) = oneshot::channel();
    FUNGI_DAEMON_CANCEL_TX.lock().unwrap().replace(tx);
    TOKIO_RUNTIME.block_on(async {
        // TODO args
        let args = DaemonArgs { fungi_dir: Some(fungi_dir), wasi_bin_path: Some(wasi_bin_path), debug_allow_all_peers: Some(true) };
        init(&args).unwrap();
        let mut daemon = FungiDaemon::new(args).await;
        daemon.start().await;
        log::info!("Fungi local peer id: {:?}", daemon.swarm_daemon.local_peer_id());
        rx.await.ok();
        log::info!("Fungi daemon stopped");
    });
}

fn stop_fungi_daemon() {
    if let Some(tx) = FUNGI_DAEMON_CANCEL_TX.lock().unwrap().take() {
        tx.send(()).unwrap();
    }
}
