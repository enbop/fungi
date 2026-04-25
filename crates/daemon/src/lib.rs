mod api;
mod controls;
mod daemon;
mod node_capabilities;
pub mod runtime;
mod service_control;
mod service_state;

/// Utilities for spawning ephemeral [`FungiDaemon`] instances in tests.
///
/// Always compiled for integration-test discoverability. External crates can depend on this crate
/// with `features = ["test-support"]` to gate their own compilation on it.
pub mod test_support;

pub use api::{ServiceAccess, ServiceAccessEndpoint};
use clap::Parser;
pub use daemon::FungiDaemon;
pub use node_capabilities::{
    LocalRuntimeAvailability, LocalRuntimeStatus, NodeAllowedTcpPorts, NodeCapabilities,
    NodePortRange, NodeRuntimeCapabilities, build_local_node_capabilities,
    build_local_runtime_status,
};
pub use runtime::{
    CatalogService, CatalogServiceEndpoint, ManifestResolutionPolicy, RuntimeControl, RuntimeKind,
    ServiceExpose, ServiceExposeEndpointBinding, ServiceExposeTransport,
    ServiceExposeTransportKind, ServiceExposeUsage, ServiceExposeUsageKind, ServiceInstance,
    ServiceLogs, ServiceLogsOptions, ServiceManifest, ServiceManifestDocument,
    ServiceManifestExpose, ServiceManifestExposeTransport, ServiceManifestExposeUsage,
    ServiceManifestHostPort, ServiceMount, ServicePort, ServicePortAllocation, ServicePortProtocol,
    ServiceSource, ServiceStatus, load_service_manifest_yaml_file, parse_service_manifest_yaml,
    service_expose_endpoint_bindings,
};
pub use service_control::{
    ServiceControlError, ServiceControlRequest, ServiceControlResponse, ServiceControlServiceRef,
};

#[derive(Debug, Clone, Default, Parser)]
pub struct DaemonArgs {
    #[clap(
        long,
        help = "Exit when stdin is closed (useful when running as a subprocess)"
    )]
    pub exit_on_stdin_close: bool,
}
