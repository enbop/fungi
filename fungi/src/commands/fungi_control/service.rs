use clap::{Subcommand, ValueEnum};
use fungi_config::FungiDir;
use fungi_daemon::{ServiceInstance, load_service_manifest_yaml_file};
use fungi_daemon_grpc::{
    Request,
    fungi_daemon_grpc::{
        DeployServiceRequest, GetServiceLogsRequest, ServiceHandleRequest, ServiceInstanceResponse,
    },
};

use crate::commands::CommonArgs;

use super::{client::get_rpc_client, shared::{fatal, fatal_grpc}};

#[derive(Subcommand, Debug, Clone)]
pub enum ServiceCommands {
    /// Deploy a service from a YAML manifest file
    Deploy {
        /// Path to a service manifest YAML file
        manifest: String,
    },
    /// Start a deployed service by runtime and handle/name
    Start {
        #[arg(value_enum)]
        runtime: ServiceRuntimeArg,
        handle: String,
    },
    /// Inspect a deployed service by runtime and handle/name
    Inspect {
        #[arg(value_enum)]
        runtime: ServiceRuntimeArg,
        handle: String,
    },
    /// Get service logs by runtime and handle/name
    Logs {
        #[arg(value_enum)]
        runtime: ServiceRuntimeArg,
        handle: String,
        #[arg(long)]
        tail: Option<String>,
    },
    /// Stop a deployed service by runtime and handle/name
    Stop {
        #[arg(value_enum)]
        runtime: ServiceRuntimeArg,
        handle: String,
    },
    /// Remove a deployed service by runtime and handle/name
    Remove {
        #[arg(value_enum)]
        runtime: ServiceRuntimeArg,
        handle: String,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ServiceRuntimeArg {
    Docker,
    Wasmtime,
}

impl ServiceRuntimeArg {
    fn to_proto_value(self) -> i32 {
        match self {
            Self::Docker => 1,
            Self::Wasmtime => 2,
        }
    }
}

pub async fn execute_service(args: CommonArgs, cmd: ServiceCommands) {
    let mut client = match get_rpc_client(&args).await {
        Some(c) => c,
        None => fatal("Cannot connect to Fungi daemon. Is it running?"),
    };

    match cmd {
        ServiceCommands::Deploy { manifest } => {
            let manifest_path = std::path::PathBuf::from(&manifest);
            let loaded = match load_service_manifest_yaml_file(&manifest_path, &args.fungi_dir()) {
                Ok(value) => value,
                Err(error) => fatal(format!("Failed to load manifest: {error}")),
            };

            let manifest_json = match serde_json::to_string(&loaded) {
                Ok(value) => value,
                Err(error) => fatal(format!("Failed to serialize manifest: {error}")),
            };

            let req = DeployServiceRequest { manifest_json };
            match client.deploy_service(Request::new(req)).await {
                Ok(resp) => print_service_instance(resp.into_inner()),
                Err(e) => fatal_grpc(e),
            }
        }
        ServiceCommands::Start { runtime, handle } => {
            let req = ServiceHandleRequest {
                runtime: runtime.to_proto_value(),
                handle,
            };
            match client.start_service(Request::new(req)).await {
                Ok(_) => println!("Service started"),
                Err(e) => fatal_grpc(e),
            }
        }
        ServiceCommands::Inspect { runtime, handle } => {
            let req = ServiceHandleRequest {
                runtime: runtime.to_proto_value(),
                handle,
            };
            match client.inspect_service(Request::new(req)).await {
                Ok(resp) => print_service_instance(resp.into_inner()),
                Err(e) => fatal_grpc(e),
            }
        }
        ServiceCommands::Logs {
            runtime,
            handle,
            tail,
        } => {
            let req = GetServiceLogsRequest {
                runtime: runtime.to_proto_value(),
                handle,
                tail: tail.unwrap_or_default(),
            };
            match client.get_service_logs(Request::new(req)).await {
                Ok(resp) => {
                    let logs = resp.into_inner();
                    print!("{}", logs.text);
                }
                Err(e) => fatal_grpc(e),
            }
        }
        ServiceCommands::Stop { runtime, handle } => {
            let req = ServiceHandleRequest {
                runtime: runtime.to_proto_value(),
                handle,
            };
            match client.stop_service(Request::new(req)).await {
                Ok(_) => println!("Service stopped"),
                Err(e) => fatal_grpc(e),
            }
        }
        ServiceCommands::Remove { runtime, handle } => {
            let req = ServiceHandleRequest {
                runtime: runtime.to_proto_value(),
                handle,
            };
            match client.remove_service(Request::new(req)).await {
                Ok(_) => println!("Service removed"),
                Err(e) => fatal_grpc(e),
            }
        }
    }
}

fn print_service_instance(resp: ServiceInstanceResponse) {
    match serde_json::from_str::<ServiceInstance>(&resp.instance_json) {
        Ok(instance) => match serde_json::to_string_pretty(&instance) {
            Ok(pretty) => println!("{}", pretty),
            Err(error) => fatal(format!("Failed to format service instance: {error}")),
        },
        Err(error) => fatal(format!("Failed to decode service instance: {error}")),
    }
}