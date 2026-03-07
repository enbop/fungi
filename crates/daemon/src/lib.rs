mod api;
mod controls;
mod daemon;
pub mod runtime;

use clap::Parser;
pub use daemon::FungiDaemon;
pub use runtime::{
    DiscoveredService, RuntimeControl, RuntimeKind, ServiceExpose, ServiceExposeTransport,
    ServiceExposeTransportKind, ServiceExposeUsage, ServiceExposeUsageKind, ServiceInstance,
    ServiceLogs, ServiceLogsOptions, ServiceManifest, ServiceManifestDocument,
    ServiceManifestExpose, ServiceManifestExposeTransport, ServiceManifestExposeUsage,
    ServiceMount, ServicePort, ServicePortProtocol, ServiceSource, ServiceStatus,
    load_service_manifest_yaml_file, parse_service_manifest_yaml,
};

#[derive(Debug, Clone, Default, Parser)]
pub struct DaemonArgs {
    #[clap(
        long,
        help = "Exit when stdin is closed (useful when running as a subprocess)"
    )]
    pub exit_on_stdin_close: bool,
}
