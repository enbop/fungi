mod api;
mod connectivity;
mod control;
mod controls;
mod daemon;
mod devices;
mod http_client;
mod inbound_access;
mod node_capabilities;
mod recipes;
pub mod runtime;
mod service_accesses;
mod service_control;
mod service_endpoints;
mod service_state;
mod services;
mod settings;

/// Utilities for spawning ephemeral [`FungiDaemon`] instances in tests.
///
/// Always compiled for integration-test discoverability. External crates can depend on this crate
/// with `features = ["test-support"]` to gate their own compilation on it.
pub mod test_support;

pub use api::{ServiceAccess, ServiceAccessEndpoint};
use clap::Parser;
pub use connectivity::Connectivity;
pub use control::FungiControl;
pub use controls::{DEFAULT_REMOTE_SERVICE_LOG_TAIL, MAX_REMOTE_SERVICE_LOG_TAIL};
pub use daemon::FungiDaemon;
pub use devices::{DeviceHandle, DeviceKind, Devices};
pub use inbound_access::InboundAccessPolicy;
pub use node_capabilities::{
    LocalRuntimeAvailability, LocalRuntimeStatus, NodeCapabilities, NodeRuntimeCapabilities,
    build_local_node_capabilities, build_local_runtime_status,
};
pub use recipes::{
    ResolvedServiceRecipe, ServiceRecipeDetail, ServiceRecipeRuntime, ServiceRecipeSummary,
};
pub use runtime::{
    AppliedService, DeviceService, DeviceServiceEndpoint, DeviceServiceMetadata,
    DeviceServiceSnapshot, ManifestResolutionPolicy, RuntimeControl, RuntimeKind,
    ServiceApplyFailure, ServiceApplyFailureStage, ServiceApplyOutcome, ServiceExpose,
    ServiceExposeEndpointBinding, ServiceExposeTransport, ServiceExposeTransportKind,
    ServiceExposeUsage, ServiceExposeUsageKind, ServiceInstance, ServiceLogs, ServiceLogsOptions,
    ServiceManifest, ServiceManifestChange, ServiceMount, ServicePhase, ServicePort,
    ServicePortAllocation, ServicePortProtocol, ServiceRunMode, ServiceSource, ServiceStatus,
    ServiceWorkloadAction, load_service_manifest_yaml_file, parse_service_manifest_yaml,
    peek_service_manifest_name, service_expose_endpoint_bindings,
    service_manifest_with_instance_name,
};
pub use service_accesses::ServiceAccesses;
pub use service_control::{
    ServiceControlError, ServiceControlRequest, ServiceControlResponse, ServiceControlServiceRef,
};
pub use services::{DeviceServices, ServiceHandle, ServiceKey, Services};
pub use settings::Settings;

#[derive(Debug, Clone, Default, Parser)]
pub struct DaemonArgs {
    #[clap(
        long,
        help = "Exit when stdin is closed (useful when running as a subprocess)"
    )]
    pub exit_on_stdin_close: bool,
}
