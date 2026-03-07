mod api;
mod controls;
mod daemon;
pub mod runtime;

use clap::Parser;
pub use daemon::FungiDaemon;
pub use runtime::{
    RuntimeControl, RuntimeKind, ServiceInstance, ServiceLogs, ServiceLogsOptions, ServiceManifest,
    ServiceMount, ServicePort, ServiceSource, ServiceStatus,
};

#[derive(Debug, Clone, Default, Parser)]
pub struct DaemonArgs {
    #[clap(
        long,
        help = "Exit when stdin is closed (useful when running as a subprocess)"
    )]
    pub exit_on_stdin_close: bool,
}
