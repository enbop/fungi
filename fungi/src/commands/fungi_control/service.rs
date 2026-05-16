#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
use std::process::Command;
use std::{
    collections::BTreeSet,
    io::{self, Write},
};

use clap::{Args, Subcommand};
use fungi_config::{FungiConfig, FungiDir, paths::FungiPaths, service_cache::ServiceCache};
use fungi_daemon::{
    CatalogService, RuntimeKind, ServiceAccess, ServiceExposeUsageKind, ServiceInstance,
    ServicePortProtocol, parse_service_manifest_yaml, service_manifest_with_instance_name,
};
use fungi_daemon_grpc::{
    Request,
    fungi_daemon_grpc::{
        AttachServiceAccessRequest, DetachServiceAccessRequest, DeviceInfo, Empty,
        GetRecipeRequest, GetServiceLogsRequest, ListDeviceServicesRequest, ListRecipesRequest,
        ListRecipesResponse, ListServiceAccessesRequest, ListServicesResponse, PullServiceRequest,
        RecipeDetail, RecipeRuntimeKind, RecipeSummary, RemotePullServiceRequest,
        RemoteServiceControlResponse, RemoteServiceNameRequest, ResolveRecipeRequest,
        ServiceInstanceResponse, ServiceNameRequest,
    },
};
use serde::Serialize;

use crate::commands::CommonArgs;

use super::{
    client::get_rpc_client,
    shared::{
        DeviceInput, OptionalDeviceTargetArg, fatal, fatal_grpc, print_target_device,
        resolve_optional_device,
    },
};

type RpcClient = fungi_daemon_grpc::fungi_daemon_grpc::fungi_daemon_client::FungiDaemonClient<
    tonic::transport::Channel,
>;
type RemoteService = CatalogService;

const SERVICE_APPLY_USAGE: &str = "Use `fungi service apply <name[@device]> <file>`, `fungi service apply <name[@device]> --recipe <id>`, or `fungi service apply --create` to create interactively.";

#[derive(Args, Debug, Clone)]
pub struct ServiceArgs {
    #[command(flatten)]
    pub device: OptionalDeviceTargetArg,
    /// Refresh remote service list from saved devices
    #[arg(long, default_value_t = false)]
    pub refresh: bool,
    #[command(subcommand)]
    pub command: Option<ServiceCommands>,
}

#[derive(Subcommand, Debug, Clone)]
pub enum ServiceCommands {
    /// List services on this device or another device
    List {
        /// Show detailed output
        #[arg(short, long, default_value_t = false)]
        verbose: bool,
        /// Refresh remote service list from saved devices
        #[arg(long, default_value_t = false)]
        refresh: bool,
    },
    /// Apply a service file to this device or another device
    Apply {
        /// Service instance target to create or update
        #[arg(value_name = "NAME[@DEVICE]")]
        target: Option<String>,
        /// Path to a .fungi.md service file
        #[arg(value_name = "SERVICE_FILE", conflicts_with_all = ["recipe", "create"])]
        manifest: Option<String>,
        /// Apply a service from an official recipe ID instead of a local file
        #[arg(long, conflicts_with_all = ["manifest", "create"])]
        recipe: Option<String>,
        /// Create a simple service interactively
        #[arg(long, conflicts_with_all = ["manifest", "recipe"], default_value_t = false)]
        create: bool,
        /// Refresh the official recipe index before resolving the recipe
        #[arg(long, default_value_t = false)]
        refresh: bool,
        /// Preview parsing, validation, and runtime intent without changing state
        #[arg(long, default_value_t = false)]
        dry_run: bool,
        /// Start the service after applying it
        #[arg(long, default_value_t = false)]
        start: bool,
        /// Skip service apply confirmation prompts
        #[arg(long, default_value_t = false)]
        yes: bool,
    },
    /// Inspect official service recipes managed by the local daemon
    Recipe {
        #[command(subcommand)]
        command: ServiceRecipeCommands,
    },
    /// Open a service in the default local app when possible
    Open {
        service: String,
        entry: Option<String>,
    },
    /// Print or create a local connection address for a service
    Connect {
        service: String,
        entry: Option<String>,
        /// Pin or move the local forwarding port for this service entry
        #[arg(long)]
        local_port: Option<u16>,
    },
    /// Remove the local persistent access for a remote service
    Disconnect { service: String },
    /// Change a service setting
    Set { service: String, setting: String },
    /// Start a service by name on this device or another device
    Start { name: String },
    /// Stop a service by name on this device or another device
    Stop { name: String },
    /// Inspect a service by name on this device or another device
    Inspect {
        name: String,
        /// Show detailed output
        #[arg(short, long, default_value_t = false)]
        verbose: bool,
    },
    /// Get local service logs by name
    Logs {
        name: String,
        #[arg(long)]
        tail: Option<String>,
    },
    /// Remove a service by name from this device or another device
    Remove {
        name: String,
        /// Remove only the local cached record for a device service
        #[arg(long, default_value_t = false)]
        local_only: bool,
        /// Confirm local-only fallback without prompting
        #[arg(short, long, default_value_t = false)]
        yes: bool,
    },
    /// Deprecated: pull a service manifest onto the local node; use `service apply`
    #[command(hide = true)]
    Pull {
        /// Path to a service manifest YAML file
        manifest: String,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum ServiceRecipeCommands {
    /// List official service recipes known to the local daemon
    List {
        /// Refresh the official recipe index before listing
        #[arg(long, default_value_t = false)]
        refresh: bool,
    },
    /// Show detailed metadata and audit paths for one official recipe
    Show {
        recipe: String,
        /// Refresh the official recipe index before showing the recipe
        #[arg(long, default_value_t = false)]
        refresh: bool,
    },
}

pub async fn execute_service(args: CommonArgs, service_args: ServiceArgs) {
    let command = service_args
        .command
        .clone()
        .unwrap_or(ServiceCommands::List {
            verbose: false,
            refresh: false,
        });
    validate_service_command_before_connect(&command);

    let mut client = match get_rpc_client(&args).await {
        Some(c) => c,
        None => fatal("Cannot connect to Fungi daemon. Is it running?"),
    };

    let device = match resolve_optional_device(&args, service_args.device.device.as_ref()) {
        Ok(device) => device,
        Err(error) => fatal(error),
    };

    match command {
        ServiceCommands::List { verbose, refresh } => {
            if let Some(device) = device {
                print_target_device(&device);
                let req = ListDeviceServicesRequest {
                    device_id: device.peer_id,
                    cached: false,
                };
                match client.list_device_managed_services(Request::new(req)).await {
                    Ok(resp) => print_service_instances(resp.into_inner(), verbose),
                    Err(error) => fatal_grpc(error),
                }
            } else {
                print_service_overview(
                    &args,
                    &mut client,
                    verbose,
                    service_args.refresh || refresh,
                )
                .await;
            }
        }
        ServiceCommands::Apply {
            target,
            manifest,
            recipe,
            create,
            refresh,
            dry_run,
            start,
            yes,
        } => {
            if !create && target.is_none() {
                fatal(service_apply_missing_target_message());
            }
            if manifest.is_some() && recipe.is_some() {
                fatal(
                    "`fungi service apply` accepts either a service file path or --recipe <id>, not both",
                );
            }
            if create {
                if manifest.is_some() || recipe.is_some() {
                    fatal(
                        "`fungi service apply --create` cannot be combined with a service file or --recipe <id>",
                    );
                }
                let target = target.map(parse_service_reference);
                let (service_name, target_device, mut created) =
                    create_tcp_service_interactively(target, start);
                let device = resolve_service_device_target(&args, device, target_device);
                apply_manifest_instance_name(&mut created, &service_name);
                if dry_run {
                    print_service_apply_dry_run(&created, &args);
                    return;
                }
                apply_created_service(&mut client, &args, device, created, yes).await;
                return;
            }
            let target = parse_service_reference(target.expect("apply target is present"));
            reject_service_entry(&target, "apply");
            let device = resolve_service_device_target(&args, device, target.device);
            let service_name = target.name;
            if let Some(recipe_id) = recipe {
                apply_service_from_recipe(
                    &mut client,
                    &args,
                    device,
                    service_name,
                    recipe_id,
                    refresh,
                    dry_run,
                    start,
                    yes,
                )
                .await;
                return;
            }
            let Some(manifest_path) = manifest else {
                fatal(service_apply_missing_source_message(None));
            };
            let mut created = read_manifest_yaml_file(&manifest_path);
            created.start_now = start;
            apply_manifest_instance_name(&mut created, &service_name);
            if dry_run {
                print_service_apply_dry_run(&created, &args);
                return;
            }
            apply_created_service(&mut client, &args, device, created, yes).await;
        }
        ServiceCommands::Recipe { command } => {
            if device.is_some() {
                fatal("Recipe commands are local-only. Run them without --device.")
            }
            match command {
                ServiceRecipeCommands::List { refresh } => {
                    print_service_recipes(&mut client, refresh).await
                }
                ServiceRecipeCommands::Show { recipe, refresh } => {
                    print_service_recipe_detail(&mut client, &recipe, refresh).await
                }
            }
        }
        ServiceCommands::Pull { manifest } => {
            let created = read_manifest_yaml_file(&manifest);
            let req = PullServiceRequest {
                manifest_yaml: created.manifest_yaml,
                manifest_base_dir: created.manifest_base_dir,
            };
            match client.pull_service(Request::new(req)).await {
                Ok(resp) => print_service_instance(resp.into_inner(), false),
                Err(e) => fatal_grpc(e),
            }
        }
        ServiceCommands::Start { name } => {
            let target = parse_service_reference(name);
            reject_service_entry(&target, "start");
            let device = resolve_service_device_target(&args, device, target.device);
            if let Some(device) = device {
                print_target_device(&device);
                let req = RemoteServiceNameRequest {
                    peer_id: device.peer_id.clone(),
                    name: target.name,
                };
                match client.remote_start_service(Request::new(req)).await {
                    Ok(resp) => {
                        print_remote_service_result("started", resp.into_inner());
                        refresh_remote_device_services(&mut client, &device.peer_id).await;
                    }
                    Err(error) => fatal_grpc(error),
                }
            } else {
                let req = ServiceNameRequest {
                    runtime: 0,
                    name: target.name,
                };
                match client.start_service(Request::new(req)).await {
                    Ok(_) => println!("Service started"),
                    Err(e) => fatal_grpc(e),
                }
            }
        }
        ServiceCommands::Inspect { name, verbose } => {
            let target = parse_service_reference(name);
            reject_service_entry(&target, "inspect");
            let device = resolve_service_device_target(&args, device, target.device);
            if let Some(device) = device {
                print_target_device(&device);
                let instance =
                    inspect_remote_service(&mut client, &device.peer_id, target.name).await;
                print_service_instance_value(instance, verbose);
            } else {
                let req = ServiceNameRequest {
                    runtime: 0,
                    name: target.name,
                };
                match client.inspect_service(Request::new(req)).await {
                    Ok(resp) => print_service_instance(resp.into_inner(), verbose),
                    Err(e) => fatal_grpc(e),
                }
            }
        }
        ServiceCommands::Logs { name, tail } => {
            if device.is_some() {
                fatal("Remote service logs are not implemented yet")
            }
            let req = GetServiceLogsRequest {
                runtime: 0,
                name,
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
        ServiceCommands::Stop { name } => {
            let target = parse_service_reference(name);
            reject_service_entry(&target, "stop");
            let device = resolve_service_device_target(&args, device, target.device);
            if let Some(device) = device {
                print_target_device(&device);
                let req = RemoteServiceNameRequest {
                    peer_id: device.peer_id.clone(),
                    name: target.name,
                };
                match client.remote_stop_service(Request::new(req)).await {
                    Ok(resp) => {
                        print_remote_service_result("stopped", resp.into_inner());
                        refresh_remote_device_services(&mut client, &device.peer_id).await;
                    }
                    Err(error) => fatal_grpc(error),
                }
            } else {
                let req = ServiceNameRequest {
                    runtime: 0,
                    name: target.name,
                };
                match client.stop_service(Request::new(req)).await {
                    Ok(_) => println!("Service stopped"),
                    Err(e) => fatal_grpc(e),
                }
            }
        }
        ServiceCommands::Remove {
            name,
            local_only,
            yes,
        } => {
            let target = parse_service_reference(name);
            reject_service_entry(&target, "remove");
            let device = resolve_service_device_target(&args, device, target.device);
            if let Some(device) = device {
                print_target_device(&device);
                let req = RemoteServiceNameRequest {
                    peer_id: device.peer_id.clone(),
                    name: target.name.clone(),
                };
                if local_only {
                    match client.forget_device_service(Request::new(req)).await {
                        Ok(resp) => {
                            print_remote_service_result("forgotten locally", resp.into_inner())
                        }
                        Err(error) => fatal_grpc(error),
                    }
                    return;
                }

                match client
                    .remote_remove_service(Request::new(req.clone()))
                    .await
                {
                    Ok(resp) => {
                        print_remote_service_result("removed", resp.into_inner());
                        refresh_remote_device_services(&mut client, &device.peer_id).await;
                    }
                    Err(error) => {
                        if is_remote_device_reachability_error(&error)
                            && cached_device_service_exists(
                                &mut client,
                                &device.peer_id,
                                &target.name,
                            )
                            .await
                        {
                            eprintln!(
                                "Cannot reach device \"{}\". This service may still exist on that device.",
                                resolved_device_display_name(&device)
                            );
                            let forget = yes
                                || prompt_yes_no_default(
                                    &format!(
                                        "Remove the local cached record for {}@{}? [y/N]",
                                        target.name,
                                        resolved_device_display_name(&device)
                                    ),
                                    false,
                                );
                            if forget {
                                match client.forget_device_service(Request::new(req)).await {
                                    Ok(resp) => {
                                        print_remote_service_result(
                                            "forgotten locally",
                                            resp.into_inner(),
                                        );
                                        return;
                                    }
                                    Err(forget_error) => fatal_grpc(forget_error),
                                }
                            }
                        }
                        fatal_grpc(error)
                    }
                }
            } else {
                let req = ServiceNameRequest {
                    runtime: 0,
                    name: target.name,
                };
                match client.remove_service(Request::new(req)).await {
                    Ok(_) => println!("Service removed"),
                    Err(e) => fatal_grpc(e),
                }
            }
        }
        ServiceCommands::Open { service, entry } => {
            let target = parse_service_reference(service);
            let entry = merge_entry(target.entry.as_deref(), entry.as_deref(), "open");
            let device = resolve_service_device_target(&args, device, target.device);
            if let Some(device) = device {
                print_target_device(&device);
                let service_ref = remote_service_reference(&target.name, &device);
                let remote_service = discover_remote_service(
                    &mut client,
                    &device.peer_id,
                    &target.name,
                    &service_ref,
                )
                .await;
                let access = existing_or_attach_access(
                    &mut client,
                    &device.peer_id,
                    &target.name,
                    None,
                    None,
                )
                .await;
                let device_name = resolved_device_display_name(&device);
                open_or_print_remote_service(
                    &remote_service,
                    &access,
                    &device_name,
                    entry.as_deref(),
                );
            } else {
                let instance = inspect_local_service(&mut client, target.name).await;
                open_or_print_local_service(&instance, entry.as_deref());
            }
        }
        ServiceCommands::Connect {
            service,
            entry,
            local_port,
        } => {
            let target = parse_service_reference(service);
            let entry = merge_entry(target.entry.as_deref(), entry.as_deref(), "connect");
            let device = resolve_service_device_target(&args, device, target.device);
            let address = if let Some(device) = device {
                print_target_device(&device);
                let service_ref = remote_service_reference(&target.name, &device);
                discover_remote_service(&mut client, &device.peer_id, &target.name, &service_ref)
                    .await;
                let access = existing_or_attach_access(
                    &mut client,
                    &device.peer_id,
                    &target.name,
                    entry.as_deref(),
                    local_port,
                )
                .await;
                select_access_endpoint(&access, entry.as_deref())
                    .map(|endpoint| format!("{}:{}", endpoint.local_host, endpoint.local_port))
            } else {
                if local_port.is_some() {
                    fatal(
                        "--local-port can only be used when connecting to a service on another device",
                    )
                }
                let instance = inspect_local_service(&mut client, target.name).await;
                select_local_port(&instance, entry.as_deref())
                    .map(|port| format!("127.0.0.1:{}", port.host_port))
            };

            let Some(address) = address else {
                fatal("No connectable entry is available for this service")
            };
            println!("{address}");
        }
        ServiceCommands::Disconnect { service } => {
            let (service, _entry, device) =
                resolve_remote_service_reference(&args, device, service, false, "disconnect");

            let req = DetachServiceAccessRequest {
                peer_id: device.peer_id,
                service_name: service,
            };
            match client.detach_service_access(Request::new(req)).await {
                Ok(_) => println!("Local access disconnected"),
                Err(error) => fatal_grpc(error),
            }
        }
        ServiceCommands::Set { service, setting } => {
            let local_port = parse_local_port_setting(&setting);
            let (service, entry, device) =
                resolve_remote_service_reference(&args, device, service, true, "set");
            print_target_device(&device);
            let service_ref = remote_service_reference(&service, &device);
            let remote_service =
                discover_remote_service(&mut client, &device.peer_id, &service, &service_ref).await;
            let access = existing_or_attach_access(
                &mut client,
                &device.peer_id,
                &service,
                entry.as_deref(),
                Some(local_port),
            )
            .await;
            let Some(endpoint) = select_access_endpoint(&access, entry.as_deref()) else {
                fatal("No connectable entry is available for this service")
            };
            let device_name = resolved_device_display_name(&device);
            print_access_details(
                &access,
                endpoint,
                &device_name,
                remote_service.usage.as_ref(),
            );
        }
    }
}

fn validate_service_command_before_connect(command: &ServiceCommands) {
    if let ServiceCommands::Apply {
        target,
        manifest,
        recipe,
        create,
        ..
    } = command
    {
        if *create {
            return;
        }
        if target.is_none() {
            fatal(service_apply_missing_target_message());
        }
        if manifest.is_none() && recipe.is_none() {
            fatal(service_apply_missing_source_message(target.as_deref()));
        }
    }
}

fn service_apply_missing_target_message() -> String {
    format!("`fungi service apply` requires NAME[@DEVICE]. {SERVICE_APPLY_USAGE}")
}

fn service_apply_missing_source_message(target: Option<&str>) -> String {
    if target.is_some_and(looks_like_service_file_path) {
        return format!(
            "`fungi service apply <file>` is missing NAME[@DEVICE] before the file. {SERVICE_APPLY_USAGE}"
        );
    }
    format!(
        "`fungi service apply` requires both NAME[@DEVICE] and either SERVICE_FILE or --recipe <id>. {SERVICE_APPLY_USAGE}"
    )
}

fn looks_like_service_file_path(value: &str) -> bool {
    let value = value.trim();
    value.ends_with(".fungi.md")
        || value.ends_with(".yaml")
        || value.ends_with(".yml")
        || value.contains('/')
        || value.starts_with('.')
        || value.starts_with('~')
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DynamicThingInvocation {
    pub target: DynamicThingTarget,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DynamicThingTarget {
    pub name: String,
    pub device: Option<DeviceInput>,
    pub entry: Option<String>,
}

pub async fn execute_dynamic_thing(
    args: CommonArgs,
    device_context: Option<DeviceInput>,
    tokens: Vec<String>,
) {
    let invocation = parse_dynamic_thing_invocation(tokens).unwrap_or_else(|error| fatal(error));

    if invocation.target.name.starts_with(':') {
        fatal("Shortcuts are not implemented yet")
    }

    if !invocation.args.is_empty() {
        fatal("Dynamic tool execution is not implemented yet")
    }

    if device_context.is_some() && invocation.target.device.is_some() {
        fatal("Device specified twice. Use either -d <device> or thing@device.")
    }

    let device = invocation.target.device.or(device_context);
    if device.is_none() {
        open_dynamic_service_without_device(args, invocation.target.name, invocation.target.entry)
            .await;
        return;
    }

    execute_service(
        args,
        ServiceArgs {
            device: OptionalDeviceTargetArg { device },
            refresh: false,
            command: Some(ServiceCommands::Open {
                service: invocation.target.name,
                entry: invocation.target.entry,
            }),
        },
    )
    .await;
}

pub fn parse_dynamic_thing_invocation(
    mut tokens: Vec<String>,
) -> Result<DynamicThingInvocation, String> {
    if tokens.is_empty() {
        return Err("Missing thing name".to_string());
    }

    let target = parse_dynamic_thing_target(tokens.remove(0))?;
    Ok(DynamicThingInvocation {
        target,
        args: tokens,
    })
}

pub fn parse_dynamic_thing_target(value: String) -> Result<DynamicThingTarget, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("Thing name cannot be empty".to_string());
    }

    let (name_and_device, entry) = match value.split_once('/') {
        Some((head, tail)) => {
            if tail.trim().is_empty() {
                return Err("Entry name cannot be empty".to_string());
            }
            (head, Some(tail.to_string()))
        }
        None => (value, None),
    };

    let (name, device) =
        match name_and_device.split_once('@') {
            Some((name, device)) => {
                if name.trim().is_empty() {
                    return Err("Thing name cannot be empty".to_string());
                }
                if device.trim().is_empty() {
                    return Err("Device name cannot be empty".to_string());
                }
                if device.contains('@') {
                    return Err("Thing target can only include one @device suffix".to_string());
                }
                (
                    name.to_string(),
                    Some(device.parse::<DeviceInput>().map_err(|error| {
                        format!("Invalid device in thing target {value}: {error}")
                    })?),
                )
            }
            None => (name_and_device.to_string(), None),
        };

    Ok(DynamicThingTarget {
        name,
        device,
        entry,
    })
}

fn parse_service_reference(value: String) -> DynamicThingTarget {
    let target = parse_dynamic_thing_target(value).unwrap_or_else(|error| fatal(error));
    target
}

pub fn fatal_dynamic_builtin_typo(name: &str, command: &str) -> ! {
    fatal(format!(
        "No service or tool named `{name}` was found.

Hint: `{name}` looks like a built-in command typo.
Did you mean:

  fungi {command}

For dynamic services, use:

  fungi filebrowser@nas"
    ))
}
async fn apply_service_from_recipe(
    client: &mut RpcClient,
    args: &CommonArgs,
    scoped_device: Option<super::shared::ResolvedPeerTarget>,
    service_name: String,
    recipe_id: String,
    refresh: bool,
    dry_run: bool,
    start: bool,
    yes: bool,
) {
    let device = scoped_device;
    let target_device_name = device.as_ref().map(resolved_device_display_name);
    let req = ResolveRecipeRequest {
        recipe_id,
        service_name: service_name.clone(),
        peer_id: device
            .as_ref()
            .map(|device| device.peer_id.clone())
            .unwrap_or_default(),
        refresh,
    };
    eprintln!("Resolving recipe; downloading recipe assets if needed...");
    let resolved = match client.resolve_recipe(Request::new(req)).await {
        Ok(resp) => resp.into_inner(),
        Err(error) => fatal_grpc(error),
    };
    let detail = require_recipe_detail(resolved.detail);

    if dry_run {
        let created = CreatedServiceManifest {
            manifest_yaml: resolved.manifest_yaml,
            manifest_base_dir: resolved.manifest_base_dir,
            start_now: start,
        };
        print_service_apply_dry_run(&created, args);
        return;
    }

    if !yes {
        print_recipe_add_review(
            &detail,
            &service_name,
            target_device_name.as_deref(),
            &resolved.resolved_manifest_path,
            &resolved.warnings,
        );
        if !prompt_yes_no_default("Apply this service from the recipe? [Y/n]", true) {
            println!("Cancelled");
            return;
        }
    } else {
        print_recipe_warnings(&resolved.warnings);
    }

    let created_for_confirm = CreatedServiceManifest {
        manifest_yaml: resolved.manifest_yaml.clone(),
        manifest_base_dir: resolved.manifest_base_dir.clone(),
        start_now: start,
    };
    confirm_apply_if_existing(client, device.as_ref(), &created_for_confirm, args, yes).await;

    if let Some(device) = device {
        print_target_device(&device);
        print_recipe_runtime_wait_notice(&detail);
        let req = RemotePullServiceRequest {
            peer_id: device.peer_id.clone(),
            manifest_yaml: resolved.manifest_yaml,
        };
        match client.remote_pull_service(Request::new(req)).await {
            Ok(resp) => {
                let response = resp.into_inner();
                let service_name = response_service_name(&response);
                print_remote_service_applied(response);
                if start {
                    let req = RemoteServiceNameRequest {
                        peer_id: device.peer_id.clone(),
                        name: service_name.clone(),
                    };
                    match client.remote_start_service(Request::new(req)).await {
                        Ok(resp) => print_remote_service_result("started", resp.into_inner()),
                        Err(error) => fatal_grpc(error),
                    }
                }
                refresh_remote_device_services(client, &device.peer_id).await;
                print_remote_apply_next_steps(&service_name, &device, start);
            }
            Err(error) => fatal_grpc(error),
        }
    } else {
        print_recipe_runtime_wait_notice(&detail);
        let req = PullServiceRequest {
            manifest_yaml: resolved.manifest_yaml,
            manifest_base_dir: resolved.manifest_base_dir,
        };
        match client.pull_service(Request::new(req)).await {
            Ok(resp) => {
                let instance = decode_service_instance(resp.into_inner());
                let name = instance.name.clone();
                print_service_instance_value(instance, false);
                if start {
                    let req = ServiceNameRequest { runtime: 0, name };
                    match client.start_service(Request::new(req)).await {
                        Ok(_) => println!("Service started"),
                        Err(error) => fatal_grpc(error),
                    }
                }
            }
            Err(error) => fatal_grpc(error),
        }
    }
}

async fn print_service_recipes(client: &mut RpcClient, refresh: bool) {
    let req = ListRecipesRequest { refresh };
    eprintln!("Loading official service recipes; downloading the index if needed...");
    match client.list_recipes(Request::new(req)).await {
        Ok(resp) => print_service_recipe_list_value(resp.into_inner()),
        Err(error) => fatal_grpc(error),
    }
}

async fn print_service_recipe_detail(client: &mut RpcClient, recipe_id: &str, refresh: bool) {
    let req = GetRecipeRequest {
        recipe_id: recipe_id.to_string(),
        refresh,
    };
    eprintln!("Loading recipe metadata; downloading recipe assets if needed...");
    match client.get_recipe(Request::new(req)).await {
        Ok(resp) => {
            print_service_recipe_detail_value(&require_recipe_detail(resp.into_inner().detail))
        }
        Err(error) => fatal_grpc(error),
    }
}

fn require_recipe_detail(detail: Option<RecipeDetail>) -> RecipeDetail {
    detail.unwrap_or_else(|| fatal("Recipe response was missing detail payload"))
}

fn recipe_summary(detail: &RecipeDetail) -> &RecipeSummary {
    detail
        .summary
        .as_ref()
        .unwrap_or_else(|| fatal("Recipe response was missing summary payload"))
}

fn recipe_runtime_label(kind: i32) -> &'static str {
    match RecipeRuntimeKind::try_from(kind) {
        Ok(RecipeRuntimeKind::Docker) => "docker",
        Ok(RecipeRuntimeKind::Wasmtime) => "wasmtime",
        Ok(RecipeRuntimeKind::Tcp) => "tcp",
        _ => "unknown",
    }
}

fn print_service_recipe_list_value(resp: ListRecipesResponse) {
    if resp.recipes.is_empty() {
        println!("No recipes found");
        return;
    }

    for recipe in resp.recipes {
        println!(
            "{:<20} {:<9} {} [{}]",
            recipe.id,
            recipe_runtime_label(recipe.runtime),
            recipe.description,
            recipe.release_version
        );
    }
}

fn print_service_recipe_detail_value(detail: &RecipeDetail) {
    print_recipe_metadata(detail);
}

fn print_recipe_add_review(
    detail: &RecipeDetail,
    service_name: &str,
    target_device_name: Option<&str>,
    _resolved_manifest_path: &str,
    warnings: &[String],
) {
    let summary = recipe_summary(detail);
    println!("Recipe: {}", summary.id);
    println!("Description: {}", summary.description);
    println!("Runtime: {}", recipe_runtime_label(summary.runtime));
    println!("Source: {}", summary.source_label);
    println!("Release: {}", summary.release_version);
    println!("Service name: {}", service_name);
    println!("Target: {}", target_device_name.unwrap_or("this device"));
    println!(
        "Audit paths: run `fungi service recipe show {}` to inspect cached and remote recipe assets.",
        summary.id
    );
    print_recipe_warnings(warnings);
}

fn print_recipe_metadata(detail: &RecipeDetail) {
    print_recipe_metadata_with_options(detail, true);
}

fn print_recipe_metadata_with_options(detail: &RecipeDetail, include_name: bool) {
    let summary = recipe_summary(detail);
    if include_name {
        println!("Name: {}", summary.name);
    }
    println!("Description: {}", summary.description);
    println!("Runtime: {}", recipe_runtime_label(summary.runtime));
    println!("Stability: {}", summary.stability);
    println!("Source: {}", summary.source_label);
    println!("Release: {}", summary.release_version);
    if !detail.tags.is_empty() {
        println!("Tags: {}", detail.tags.join(", "));
    }
    if !detail.homepage.is_empty() {
        println!("Homepage: {}", detail.homepage);
    }
    println!("Cached manifest: {}", detail.cached_manifest_path);
    if !detail.cached_readme_path.is_empty() {
        println!("Cached readme: {}", detail.cached_readme_path);
    }
    println!("Remote manifest: {}", detail.remote_manifest_url);
    if !detail.remote_readme_url.is_empty() {
        println!("Remote readme: {}", detail.remote_readme_url);
    }
}

fn print_recipe_warnings(warnings: &[String]) {
    for warning in warnings {
        eprintln!("Warning: {warning}");
    }
}

fn print_recipe_runtime_wait_notice(detail: &RecipeDetail) {
    let summary = recipe_summary(detail);
    match RecipeRuntimeKind::try_from(summary.runtime) {
        Ok(RecipeRuntimeKind::Docker) => {
            eprintln!(
                "Preparing Docker service; the first run may take a while while the image is pulled..."
            );
        }
        Ok(RecipeRuntimeKind::Wasmtime) => {
            eprintln!(
                "Preparing Wasmtime service; downloading the component if it is not cached..."
            );
        }
        _ => {}
    }
}

fn print_remote_apply_next_steps(
    service_name: &str,
    device: &super::shared::ResolvedPeerTarget,
    started: bool,
) {
    let target = format!("{}@{}", service_name, resolved_device_display_name(device));
    if started {
        println!("Use it:");
        println!("  fungi {target}");
    } else {
        println!("Apply finished without --start.");
        println!("If this is a new or stopped service, start it when ready:");
        println!("  fungi service start {target}");
    }
}

async fn apply_created_service(
    client: &mut RpcClient,
    args: &CommonArgs,
    device: Option<super::shared::ResolvedPeerTarget>,
    created: CreatedServiceManifest,
    yes: bool,
) {
    confirm_apply_if_existing(client, device.as_ref(), &created, args, yes).await;
    if let Some(device) = device {
        print_target_device(&device);
        let req = RemotePullServiceRequest {
            peer_id: device.peer_id.clone(),
            manifest_yaml: created.manifest_yaml,
        };
        match client.remote_pull_service(Request::new(req)).await {
            Ok(resp) => {
                let response = resp.into_inner();
                let applied_service_name = response_service_name(&response);
                print_remote_service_applied(response);
                if created.start_now {
                    let req = RemoteServiceNameRequest {
                        peer_id: device.peer_id.clone(),
                        name: applied_service_name.clone(),
                    };
                    match client.remote_start_service(Request::new(req)).await {
                        Ok(resp) => print_remote_service_result("started", resp.into_inner()),
                        Err(error) => fatal_grpc(error),
                    }
                }
                refresh_remote_device_services(client, &device.peer_id).await;
                print_remote_apply_next_steps(&applied_service_name, &device, created.start_now);
            }
            Err(error) => fatal_grpc(error),
        }
    } else {
        let req = PullServiceRequest {
            manifest_yaml: created.manifest_yaml,
            manifest_base_dir: created.manifest_base_dir,
        };
        match client.pull_service(Request::new(req)).await {
            Ok(resp) => {
                let instance = decode_service_instance(resp.into_inner());
                let name = instance.name.clone();
                print_service_instance_value(instance, false);
                if created.start_now {
                    let req = ServiceNameRequest { runtime: 0, name };
                    match client.start_service(Request::new(req)).await {
                        Ok(_) => println!("Service started"),
                        Err(error) => fatal_grpc(error),
                    }
                }
            }
            Err(error) => fatal_grpc(error),
        }
    }
}

fn reject_service_entry(target: &DynamicThingTarget, action: &str) {
    if target.entry.is_some() {
        fatal(format!("Entry-specific {action} is not implemented yet"))
    }
}

fn merge_entry(primary: Option<&str>, secondary: Option<&str>, action: &str) -> Option<String> {
    match (primary, secondary) {
        (Some(_), Some(_)) => fatal(format!(
            "Entry specified twice. Use either `service@device/entry` or `fungi service {action} <service> <entry>`."
        )),
        (Some(entry), None) | (None, Some(entry)) => Some(entry.to_string()),
        (None, None) => None,
    }
}

fn resolve_service_device_target(
    args: &CommonArgs,
    scoped_device: Option<super::shared::ResolvedPeerTarget>,
    target_device: Option<DeviceInput>,
) -> Option<super::shared::ResolvedPeerTarget> {
    match (scoped_device, target_device) {
        (Some(_), Some(_)) => {
            fatal("Device specified twice. Use either --device <device> or service@device.")
        }
        (Some(device), None) => Some(device),
        (None, Some(device_input)) => match resolve_optional_device(args, Some(&device_input)) {
            Ok(device) => device,
            Err(error) => fatal(error),
        },
        (None, None) => None,
    }
}

fn resolve_remote_service_reference(
    args: &CommonArgs,
    scoped_device: Option<super::shared::ResolvedPeerTarget>,
    value: String,
    allow_entry: bool,
    action: &str,
) -> (String, Option<String>, super::shared::ResolvedPeerTarget) {
    let target = parse_service_reference(value);
    if target.entry.is_some() && !allow_entry {
        fatal(format!("Entry-specific {action} is not implemented yet"))
    }

    let device = match (scoped_device, target.device) {
        (Some(_), Some(_)) => {
            fatal("Device specified twice. Use either --device <device> or service@device.")
        }
        (Some(device), None) => device,
        (None, Some(device_input)) => match resolve_optional_device(args, Some(&device_input)) {
            Ok(Some(device)) => device,
            Ok(None) => fatal(format!("Device is required for {action}")),
            Err(error) => fatal(error),
        },
        (None, None) => fatal(format!(
            "Device is required. Use `fungi service {action} <service>@<device>`."
        )),
    };

    (target.name, target.entry, device)
}

fn parse_local_port_setting(setting: &str) -> u16 {
    let Some((key, value)) = setting.split_once('=') else {
        fatal("Setting must look like local.port=2222")
    };
    if key.trim() != "local.port" {
        fatal("Unknown setting. Supported settings: local.port")
    }

    let port = value
        .trim()
        .parse::<u16>()
        .unwrap_or_else(|_| fatal("local.port must be a number between 1 and 65535"));
    if port == 0 {
        fatal("local.port must be greater than 0")
    }
    port
}

fn response_service_name(resp: &RemoteServiceControlResponse) -> String {
    let service_name = if resp.service_name.trim().is_empty() {
        "<unknown>"
    } else {
        resp.service_name.as_str()
    };
    service_name.to_string()
}

fn print_remote_service_applied(resp: RemoteServiceControlResponse) {
    let service_name = response_service_name(&resp);
    println!("Remote service applied: {service_name}");
}

fn print_remote_service_result(action: &str, resp: RemoteServiceControlResponse) {
    let service_name = response_service_name(&resp);
    if resp.forgotten_locally {
        println!("Local service record removed: {service_name}");
        println!("The remote device was not changed; the service may still exist there.");
    } else {
        println!("Remote service {action}: {service_name}");
    }
}

fn is_remote_device_reachability_error(error: &tonic::Status) -> bool {
    let message = error.message();
    message.contains("Failed to open service-control stream")
        || message.contains("Failed to write service-control request")
        || message.contains("Failed to read service-control response")
        || message.contains("No connections available to peer")
}

async fn refresh_remote_device_services(client: &mut RpcClient, peer_id: &str) {
    if let Err(error) = fetch_remote_managed_services(client, peer_id).await {
        eprintln!("Warning: failed to refresh remote managed service cache: {error}");
    }
    if let Err(error) = fetch_remote_services(client, peer_id).await {
        eprintln!("Warning: failed to refresh remote published service cache: {error}");
    }
}

async fn inspect_local_service(client: &mut RpcClient, name: String) -> ServiceInstance {
    let req = ServiceNameRequest { runtime: 0, name };
    match client.inspect_service(Request::new(req)).await {
        Ok(resp) => match serde_json::from_str::<ServiceInstance>(&resp.into_inner().instance_json)
        {
            Ok(instance) => instance,
            Err(error) => fatal(format!("Failed to decode service instance: {error}")),
        },
        Err(error) => fatal_grpc(error),
    }
}

async fn open_dynamic_service_without_device(
    args: CommonArgs,
    service: String,
    entry: Option<String>,
) {
    let builtin_hint = if entry.is_none() {
        let tokens = [service.clone()];
        crate::commands::dynamic_builtin_typo_hint_for_tokens(&tokens, None)
            .map(|(_, command)| command)
    } else {
        None
    };

    if let Some(command) = builtin_hint.as_ref()
        && FungiConfig::try_read_from_dir(&args.fungi_dir()).is_err()
    {
        fatal_dynamic_builtin_typo(&service, command)
    }

    let mut client = match get_rpc_client(&args).await {
        Some(c) => c,
        None => {
            if let Some(command) = builtin_hint {
                fatal_dynamic_builtin_typo(&service, &command)
            }
            fatal("Cannot connect to Fungi daemon. Is it running?")
        }
    };

    if let Some(instance) = find_local_service(&mut client, &service).await {
        let Some(url) = build_local_web_url(&instance, entry.as_deref()) else {
            fatal("No web entry is available for this service")
        };
        open_url(&url);
        println!("Opened {url}");
        return;
    }

    if let Some(command) = builtin_hint {
        fatal_dynamic_builtin_typo(&service, &command)
    }

    fatal(format!(
        "Local service not found: {service}
Remote services must be addressed explicitly with `fungi <service>@<device>` or `fungi service -d <device> open <service>`."
    ));
}
async fn find_local_service(client: &mut RpcClient, service: &str) -> Option<ServiceInstance> {
    list_local_service_instances(client)
        .await
        .into_iter()
        .find(|instance| instance.name == service || instance.id == service)
}

async fn print_service_overview(
    args: &CommonArgs,
    client: &mut RpcClient,
    verbose: bool,
    refresh: bool,
) {
    let mut rows = Vec::new();

    let local_services = list_local_service_instances(client).await;
    rows.extend(
        local_services
            .into_iter()
            .map(|service| ServiceOverviewRow::from_local(service, verbose)),
    );

    let devices = list_saved_devices(client).await;
    if refresh {
        for device in devices {
            let attached = list_accesses(client, &device.peer_id).await;
            let managed = fetch_remote_managed_services(client, &device.peer_id).await;
            let published = fetch_remote_services(client, &device.peer_id).await;
            add_remote_service_overview_rows(
                &mut rows, &device, &attached, managed, published, verbose, false,
            );
        }
    } else {
        for device in devices {
            let accesses = list_accesses(client, &device.peer_id).await;
            let cached_published = load_cached_remote_services(args, &device.peer_id);
            let cached_managed = load_cached_remote_managed_services(args, &device.peer_id);
            if cached_published.is_none() && cached_managed.is_none() {
                rows.extend(accesses.into_iter().map(|access| {
                    ServiceOverviewRow::from_cached_access(access, &device, verbose)
                }));
            } else {
                add_cached_remote_service_overview_rows(
                    &mut rows,
                    &device,
                    &accesses,
                    cached_managed.unwrap_or_default(),
                    cached_published.unwrap_or_default(),
                    verbose,
                    true,
                );
            }
        }
    }

    rows.sort_by(|left, right| left.reference.cmp(&right.reference));
    print_service_overview_rows(&rows);
}

fn add_remote_service_overview_rows(
    rows: &mut Vec<ServiceOverviewRow>,
    device: &DeviceInfo,
    attached: &[ServiceAccess],
    managed: Result<Vec<ServiceInstance>, String>,
    published: Result<Vec<RemoteService>, String>,
    verbose: bool,
    cached: bool,
) {
    match (managed, published) {
        (Err(managed_error), Err(published_error)) => {
            rows.push(ServiceOverviewRow::remote_unavailable(
                device,
                format!("{managed_error}; {published_error}"),
            ));
        }
        (managed, published) => {
            let managed = managed.unwrap_or_default();
            let published = published.unwrap_or_default();
            add_cached_remote_service_overview_rows(
                rows, device, attached, managed, published, verbose, cached,
            );
        }
    }
}

fn add_cached_remote_service_overview_rows(
    rows: &mut Vec<ServiceOverviewRow>,
    device: &DeviceInfo,
    attached: &[ServiceAccess],
    managed: Vec<ServiceInstance>,
    published: Vec<RemoteService>,
    verbose: bool,
    cached: bool,
) {
    let mut published_names = BTreeSet::new();
    for service in published {
        published_names.insert(service.service_name.clone());
        rows.push(ServiceOverviewRow::from_remote_service(
            service, device, attached, verbose, cached,
        ));
    }

    for service in managed {
        if !published_names.contains(&service.name) {
            rows.push(ServiceOverviewRow::from_remote_managed_service(
                service, device, attached, verbose,
            ));
        }
    }
}

async fn list_local_service_instances(client: &mut RpcClient) -> Vec<ServiceInstance> {
    match client.list_services(Request::new(Empty {})).await {
        Ok(resp) => decode_service_instances(resp.into_inner()),
        Err(error) => fatal_grpc(error),
    }
}

async fn list_saved_devices(client: &mut RpcClient) -> Vec<DeviceInfo> {
    match client.list_devices(Request::new(Empty {})).await {
        Ok(resp) => resp.into_inner().devices,
        Err(error) => fatal_grpc(error),
    }
}

async fn list_remote_service_instances(
    client: &mut RpcClient,
    peer_id: &str,
) -> Vec<ServiceInstance> {
    match fetch_remote_managed_services(client, peer_id).await {
        Ok(services) => services,
        Err(error) => fatal(error),
    }
}

async fn fetch_remote_managed_services(
    client: &mut RpcClient,
    peer_id: &str,
) -> Result<Vec<ServiceInstance>, String> {
    let req = ListDeviceServicesRequest {
        device_id: peer_id.to_string(),
        cached: false,
    };
    match client.list_device_managed_services(Request::new(req)).await {
        Ok(resp) => serde_json::from_str::<Vec<ServiceInstance>>(&resp.into_inner().services_json)
            .map_err(|error| format!("Failed to decode remote managed services: {error}")),
        Err(error) => Err(error.message().to_string()),
    }
}

async fn fetch_remote_services(
    client: &mut RpcClient,
    peer_id: &str,
) -> Result<Vec<RemoteService>, String> {
    let req = ListDeviceServicesRequest {
        device_id: peer_id.to_string(),
        cached: false,
    };
    match client
        .list_device_published_services(Request::new(req))
        .await
    {
        Ok(resp) => serde_json::from_str::<Vec<RemoteService>>(&resp.into_inner().services_json)
            .map_err(|error| format!("Failed to decode remote services: {error}")),
        Err(error) => Err(error.message().to_string()),
    }
}

async fn fetch_cached_remote_services(client: &mut RpcClient, peer_id: &str) -> Vec<RemoteService> {
    let req = ListDeviceServicesRequest {
        device_id: peer_id.to_string(),
        cached: true,
    };
    match client
        .list_device_published_services(Request::new(req))
        .await
    {
        Ok(resp) => {
            match serde_json::from_str::<Vec<RemoteService>>(&resp.into_inner().services_json) {
                Ok(services) => services,
                Err(error) => fatal(format!("Failed to decode cached remote services: {error}")),
            }
        }
        Err(error) => fatal_grpc(error),
    }
}

async fn fetch_cached_remote_managed_services(
    client: &mut RpcClient,
    peer_id: &str,
) -> Vec<ServiceInstance> {
    let req = ListDeviceServicesRequest {
        device_id: peer_id.to_string(),
        cached: true,
    };
    match client.list_device_managed_services(Request::new(req)).await {
        Ok(resp) => {
            match serde_json::from_str::<Vec<ServiceInstance>>(&resp.into_inner().services_json) {
                Ok(services) => services,
                Err(error) => fatal(format!(
                    "Failed to decode cached remote managed services: {error}"
                )),
            }
        }
        Err(error) => fatal_grpc(error),
    }
}

fn load_cached_remote_services(args: &CommonArgs, peer_id: &str) -> Option<Vec<RemoteService>> {
    let cache = ServiceCache::apply_published_services_from_dir(&args.fungi_dir())
        .unwrap_or_else(|error| fatal(format!("Failed to load remote service cache: {error}")));
    let Some(services_json) = cache
        .get_device_services_json(peer_id)
        .unwrap_or_else(|error| fatal(format!("Failed to read remote service cache: {error}")))
    else {
        return None;
    };

    Some(
        serde_json::from_str::<Vec<RemoteService>>(&services_json).unwrap_or_else(|error| {
            fatal(format!("Failed to decode cached remote services: {error}"))
        }),
    )
}

fn load_cached_remote_managed_services(
    args: &CommonArgs,
    peer_id: &str,
) -> Option<Vec<ServiceInstance>> {
    let cache =
        ServiceCache::apply_managed_services_from_dir(&args.fungi_dir()).unwrap_or_else(|error| {
            fatal(format!(
                "Failed to load remote managed service cache: {error}"
            ))
        });
    let Some(services_json) = cache
        .get_device_services_json(peer_id)
        .unwrap_or_else(|error| {
            fatal(format!(
                "Failed to read remote managed service cache: {error}"
            ))
        })
    else {
        return None;
    };

    Some(
        serde_json::from_str::<Vec<ServiceInstance>>(&services_json).unwrap_or_else(|error| {
            fatal(format!(
                "Failed to decode cached remote managed services: {error}"
            ))
        }),
    )
}

async fn cached_device_service_exists(
    client: &mut RpcClient,
    peer_id: &str,
    service_name: &str,
) -> bool {
    fetch_cached_remote_managed_services(client, peer_id)
        .await
        .iter()
        .any(|service| service.name == service_name)
        || fetch_cached_remote_services(client, peer_id)
            .await
            .iter()
            .any(|service| service.service_name == service_name)
}

async fn inspect_remote_service(
    client: &mut RpcClient,
    peer_id: &str,
    name: String,
) -> ServiceInstance {
    list_remote_service_instances(client, peer_id)
        .await
        .into_iter()
        .find(|instance| instance.name == name)
        .unwrap_or_else(|| fatal(format!("Remote service not found: {name}")))
}

async fn list_accesses(client: &mut RpcClient, peer_id: &str) -> Vec<ServiceAccess> {
    let req = ListServiceAccessesRequest {
        peer_id: peer_id.to_string(),
    };
    match client.list_service_accesses(Request::new(req)).await {
        Ok(resp) => match serde_json::from_str::<Vec<ServiceAccess>>(
            &resp.into_inner().service_accesses_json,
        ) {
            Ok(accesses) => accesses,
            Err(error) => fatal(format!("Failed to decode access list: {error}")),
        },
        Err(error) => fatal_grpc(error),
    }
}

async fn attach_access_with_options(
    client: &mut RpcClient,
    peer_id: &str,
    service_name: &str,
    entry: Option<&str>,
    local_port: Option<u16>,
) -> ServiceAccess {
    let req = AttachServiceAccessRequest {
        peer_id: peer_id.to_string(),
        service_name: service_name.to_string(),
        entry: entry.unwrap_or_default().to_string(),
        local_port: local_port.unwrap_or_default() as i32,
    };
    match client.attach_service_access(Request::new(req)).await {
        Ok(resp) => {
            match serde_json::from_str::<ServiceAccess>(&resp.into_inner().service_access_json) {
                Ok(access) => access,
                Err(error) => fatal(format!("Failed to decode service access: {error}")),
            }
        }
        Err(error) => fatal_grpc(error),
    }
}

async fn existing_or_attach_access(
    client: &mut RpcClient,
    peer_id: &str,
    service_name: &str,
    entry: Option<&str>,
    local_port: Option<u16>,
) -> ServiceAccess {
    let existing = list_accesses(client, peer_id).await;
    if let Some(access) = existing.into_iter().find(|access| {
        access.service_name == service_name
            && local_port.is_none()
            && entry
                .map(|entry| {
                    access
                        .endpoints
                        .iter()
                        .any(|endpoint| endpoint.name == entry)
                })
                .unwrap_or(true)
    }) {
        return access;
    }

    attach_access_with_options(client, peer_id, service_name, entry, local_port).await
}

async fn discover_remote_service(
    client: &mut RpcClient,
    peer_id: &str,
    service_name: &str,
    service_ref: &str,
) -> RemoteService {
    if let Some(service) = fetch_cached_remote_services(client, peer_id)
        .await
        .into_iter()
        .find(|service| service.service_name == service_name)
    {
        return service;
    }

    let services = match fetch_remote_services(client, peer_id).await {
        Ok(services) => services,
        Err(error) => fatal(error),
    };

    if let Some(service) = services
        .into_iter()
        .find(|service| service.service_name == service_name)
    {
        return service;
    }

    let message =
        remote_service_unavailable_message(client, peer_id, service_name, service_ref).await;
    fatal(message)
}

async fn remote_service_unavailable_message(
    client: &mut RpcClient,
    peer_id: &str,
    service_name: &str,
    service_ref: &str,
) -> String {
    let managed = find_remote_managed_service(client, peer_id, service_name).await;
    let Some(managed) = managed else {
        return format!("Remote service not found: {service_name}");
    };

    if !managed.status.running {
        return format!(
            "Remote service {service_ref} exists but is not running (state: {}). Start it with:\n  fungi service start {service_ref}",
            managed.status.state
        );
    }

    format!(
        "Remote service {service_ref} is running but does not publish a connectable endpoint yet."
    )
}

async fn find_remote_managed_service(
    client: &mut RpcClient,
    peer_id: &str,
    service_name: &str,
) -> Option<ServiceInstance> {
    if let Ok(services) = fetch_remote_managed_services(client, peer_id).await
        && let Some(service) = services
            .into_iter()
            .find(|service| service.name == service_name)
    {
        return Some(service);
    }

    if let Some(service) = fetch_cached_remote_managed_services(client, peer_id)
        .await
        .into_iter()
        .find(|service| service.name == service_name)
    {
        return Some(service);
    }
    None
}

fn remote_service_reference(
    service_name: &str,
    device: &super::shared::ResolvedPeerTarget,
) -> String {
    format!("{service_name}@{}", resolved_device_display_name(device))
}

fn build_web_url(
    service: &RemoteService,
    access: &ServiceAccess,
    entry: Option<&str>,
) -> Option<String> {
    if !matches!(
        service.usage.as_ref().map(|usage| usage.kind),
        Some(ServiceExposeUsageKind::Web)
    ) {
        return None;
    }

    let endpoint = select_access_endpoint(access, entry)?;
    let mut value = format!("http://{}:{}", endpoint.local_host, endpoint.local_port);
    if let Some(path) = service
        .usage
        .as_ref()
        .and_then(|usage| usage.path.as_deref())
        && !path.is_empty()
    {
        if path.starts_with('/') {
            value.push_str(path);
        } else {
            value.push('/');
            value.push_str(path);
        }
    }
    Some(value)
}

fn open_or_print_remote_service(
    service: &RemoteService,
    access: &ServiceAccess,
    device_name: &str,
    entry: Option<&str>,
) {
    if matches!(
        service.usage.as_ref().map(|usage| usage.kind),
        Some(ServiceExposeUsageKind::Web)
    ) {
        let Some(url) = build_web_url(service, access, entry) else {
            fatal("No web entry is available for this service")
        };
        open_url(&url);
        println!("Opened {url}");
        return;
    }

    let Some(endpoint) = select_access_endpoint(access, entry) else {
        fatal("No connectable entry is available for this service")
    };
    print_access_details(access, endpoint, device_name, service.usage.as_ref());
}

fn print_access_details(
    access: &ServiceAccess,
    endpoint: &fungi_daemon::ServiceAccessEndpoint,
    device_name: &str,
    usage: Option<&fungi_daemon::ServiceExposeUsage>,
) {
    let usage = service_usage_label(usage);
    let remote_port = format_remote_port(endpoint.remote_port);
    println!("{}@{}", access.service_name, device_name);
    println!("type: {usage}");
    println!("state: connected");
    println!();
    println!("forward:");
    println!(
        "  {}  {}:{} -> {}:{}",
        endpoint.name, endpoint.local_host, endpoint.local_port, device_name, remote_port
    );
    println!();
    println!("local address:");
    println!("  {}:{}", endpoint.local_host, endpoint.local_port);
}

fn build_local_web_url(instance: &ServiceInstance, entry: Option<&str>) -> Option<String> {
    select_local_web_port(instance, entry)
        .map(|port| format!("http://127.0.0.1:{}", port.host_port))
}

fn open_or_print_local_service(instance: &ServiceInstance, entry: Option<&str>) {
    if let Some(url) = build_local_web_url(instance, entry) {
        open_url(&url);
        println!("Opened {url}");
        return;
    }

    let Some(port) = select_local_port(instance, entry) else {
        fatal("No connectable entry is available for this service")
    };
    println!("127.0.0.1:{}", port.host_port);
}

fn select_access_endpoint<'a>(
    access: &'a ServiceAccess,
    entry: Option<&str>,
) -> Option<&'a fungi_daemon::ServiceAccessEndpoint> {
    if let Some(entry) = entry {
        return access
            .endpoints
            .iter()
            .find(|endpoint| endpoint.name == entry);
    }

    access
        .endpoints
        .iter()
        .find(|endpoint| endpoint.name == "web")
        .or_else(|| {
            access
                .endpoints
                .iter()
                .find(|endpoint| endpoint.name == "main")
        })
        .or_else(|| access.endpoints.first())
}

fn select_local_port<'a>(
    instance: &'a ServiceInstance,
    entry: Option<&str>,
) -> Option<&'a fungi_daemon::ServicePort> {
    if let Some(entry) = entry {
        return instance
            .ports
            .iter()
            .find(|port| port.name.as_deref() == Some(entry));
    }

    instance
        .ports
        .iter()
        .find(|port| port.name.as_deref() == Some("web"))
        .or_else(|| {
            instance
                .ports
                .iter()
                .find(|port| port.name.as_deref() == Some("main"))
        })
        .or_else(|| instance.ports.first())
}

fn select_local_web_port<'a>(
    instance: &'a ServiceInstance,
    entry: Option<&str>,
) -> Option<&'a fungi_daemon::ServicePort> {
    if let Some(entry) = entry {
        if is_non_web_entry_name(entry) {
            return None;
        }
        return select_local_port(instance, Some(entry));
    }

    instance
        .ports
        .iter()
        .find(|port| is_web_entry_name(port.name.as_deref()))
}

fn is_web_entry_name(name: Option<&str>) -> bool {
    matches!(
        name.map(|value| value.trim().to_ascii_lowercase()),
        Some(value) if matches!(value.as_str(), "web" | "http" | "https")
    )
}

fn is_non_web_entry_name(name: &str) -> bool {
    matches!(
        name.trim().to_ascii_lowercase().as_str(),
        "ssh" | "tcp" | "raw" | "api" | "mcp"
    )
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn run_url_opener(mut command: Command) {
    match command.status() {
        Ok(result) if result.success() => {}
        Ok(result) => fatal(format!("Failed to open URL, exit code: {result}")),
        Err(error) => fatal(format!("Failed to launch URL opener: {error}")),
    }
}

#[cfg(target_os = "macos")]
fn open_url(url: &str) {
    let mut command = Command::new("open");
    command.arg(url);
    run_url_opener(command);
}

#[cfg(target_os = "linux")]
fn open_url(url: &str) {
    let mut command = Command::new("xdg-open");
    command.arg(url);
    run_url_opener(command);
}

#[cfg(target_os = "windows")]
fn open_url(url: &str) {
    let mut command = Command::new("cmd");
    command.args(["/C", "start", "", url]);
    run_url_opener(command);
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn open_url(_url: &str) {
    fatal("Opening URLs is not supported on this platform")
}

#[derive(Debug, Clone)]
pub(crate) struct CreatedServiceManifest {
    pub(crate) manifest_yaml: String,
    pub(crate) manifest_base_dir: String,
    start_now: bool,
}

pub(crate) fn read_manifest_yaml_file(path: &str) -> CreatedServiceManifest {
    let manifest_path = std::path::PathBuf::from(path);
    let absolute_manifest_path = match std::fs::canonicalize(&manifest_path) {
        Ok(path) => path,
        Err(error) => fatal(format!("Failed to resolve manifest path: {error}")),
    };
    let manifest_yaml = match std::fs::read_to_string(&absolute_manifest_path) {
        Ok(value) => value,
        Err(error) => fatal(format!("Failed to read manifest: {error}")),
    };
    let manifest_base_dir = absolute_manifest_path
        .parent()
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_default();
    CreatedServiceManifest {
        manifest_yaml,
        manifest_base_dir,
        start_now: false,
    }
}

fn create_tcp_service_interactively(
    target: Option<DynamicThingTarget>,
    start_flag: bool,
) -> (String, Option<DeviceInput>, CreatedServiceManifest) {
    if let Some(target) = &target {
        reject_service_entry(target, "apply");
    }
    let service_name = match target.as_ref() {
        Some(target) => target.name.clone(),
        None => prompt_required("Service instance name"),
    };
    let target_device = match target {
        Some(target) => target.device,
        None => {
            let value = prompt("Target device (blank for this device)");
            let value = value.trim();
            if value.is_empty() {
                None
            } else {
                Some(
                    value
                        .parse::<DeviceInput>()
                        .unwrap_or_else(|error| fatal(format!("Invalid target device: {error}"))),
                )
            }
        }
    };
    let host = prompt_with_default("Local host on target device", "127.0.0.1");
    let port = prompt_required("Local TCP port")
        .parse::<u16>()
        .unwrap_or_else(|error| fatal(format!("Invalid TCP port: {error}")));
    if port == 0 {
        fatal("Local TCP port must be greater than 0")
    }
    let client_kind = prompt_with_default("Client kind (raw, ssh, web, webdav)", "raw");
    let default_entry = match client_kind.trim().to_ascii_lowercase().as_str() {
        "ssh" => "ssh",
        "web" => "http",
        "webdav" => "webdav",
        _ => "main",
    };
    let entry = prompt_with_default("Entry name", default_entry);
    let client_path = if client_kind.trim().eq_ignore_ascii_case("web") {
        Some(prompt_with_default("Web path", "/"))
    } else {
        None
    };
    let start_now = start_flag || prompt_yes_no_default("Start after apply? [Y/n]", true);
    let mut manifest_yaml = format!(
        "fungi: service/v1\nid: external-tcp\n\npublish:\n  {}:\n    tcp:\n      host: {}\n      port: {}\n    client:\n      kind: {}\n",
        yaml_key(&entry),
        yaml_scalar(&host),
        port,
        yaml_scalar(&client_kind)
    );
    if let Some(path) = client_path {
        manifest_yaml.push_str(&format!("      path: {}\n", yaml_scalar(&path)));
    }

    (
        service_name,
        target_device,
        CreatedServiceManifest {
            manifest_yaml,
            manifest_base_dir: String::new(),
            start_now,
        },
    )
}

fn manifest_base_dir_path(created: &CreatedServiceManifest) -> std::path::PathBuf {
    if created.manifest_base_dir.trim().is_empty() {
        std::path::PathBuf::from(".")
    } else {
        std::path::PathBuf::from(&created.manifest_base_dir)
    }
}

fn print_service_apply_dry_run(created: &CreatedServiceManifest, args: &CommonArgs) {
    let base_dir = manifest_base_dir_path(created);
    let fungi_home = manifest_parse_fungi_home(args);
    let manifest = parse_service_manifest_yaml(&created.manifest_yaml, &base_dir, &fungi_home)
        .unwrap_or_else(|error| fatal(format!("Failed to parse service manifest: {error}")));
    let mut warnings = Vec::new();

    println!("Service: {}", manifest.name);
    println!("Run:");
    if manifest.runtime == RuntimeKind::External {
        println!("  none");
    } else {
        println!("  runtime: {}", runtime_kind_label(manifest.runtime));
    }
    if manifest.runtime == RuntimeKind::Wasmtime {
        println!("  mode: {}", wasmtime_run_mode_label(manifest.run_mode));
        println!(
            "  invocation: {}",
            if manifest.run_mode == fungi_daemon::ServiceRunMode::Http {
                "serve"
            } else {
                "run"
            }
        );
    }
    if !manifest.mounts.is_empty() {
        println!("Mounts:");
        let user_root = FungiPaths::from_fungi_home(&fungi_home).user_root();
        for mount in &manifest.mounts {
            println!("  {} -> {}", mount.host_path.display(), mount.runtime_path);
            if mount.host_path == user_root {
                warnings.push(
                    "$fungi.root exposes the full Fungi user root; prefer $fungi.workspace when possible."
                        .to_string(),
                );
            }
        }
    }
    println!("Publish:");
    for port in &manifest.ports {
        let name = port.name.as_deref().unwrap_or("main");
        println!(
            "  {name}: tcp service:{} daemon:{} ({})",
            port.service_port,
            port.host_port,
            match port.host_port_allocation {
                fungi_daemon::ServicePortAllocation::Auto => "auto",
                fungi_daemon::ServicePortAllocation::Fixed => "fixed",
            }
        );
    }
    if manifest.runtime == RuntimeKind::Wasmtime
        && manifest
            .ports
            .iter()
            .any(|port| port.protocol == ServicePortProtocol::Tcp)
    {
        println!("Runtime grants:");
        println!("  - tcp");
        println!("  - inherited network");
        println!("  - DNS lookup");
        warnings
            .push("Wasmtime TCP services currently receive broad host network access.".to_string());
    }
    if !warnings.is_empty() {
        println!("Warnings:");
        for warning in warnings {
            println!("  - {warning}");
        }
    }
    if created.start_now {
        println!("After apply: start service");
    } else {
        println!("After apply: leave service stopped unless it was already running");
    }
}

fn wasmtime_run_mode_label(mode: fungi_daemon::ServiceRunMode) -> &'static str {
    match mode {
        fungi_daemon::ServiceRunMode::Command => "command (default)",
        fungi_daemon::ServiceRunMode::Http => "http",
    }
}

fn apply_manifest_instance_name(created: &mut CreatedServiceManifest, service_name: &str) {
    created.manifest_yaml =
        service_manifest_with_instance_name(&created.manifest_yaml, service_name)
            .unwrap_or_else(|error| fatal(format!("Failed to set service instance name: {error}")));
}

fn manifest_apply_identity(
    created: &CreatedServiceManifest,
    args: &CommonArgs,
) -> (String, RuntimeKind, Option<String>) {
    let base_dir = manifest_base_dir_path(created);
    let fungi_home = manifest_parse_fungi_home(args);
    let manifest = parse_service_manifest_yaml(&created.manifest_yaml, &base_dir, &fungi_home)
        .unwrap_or_else(|error| fatal(format!("Failed to parse service manifest: {error}")));
    (manifest.name, manifest.runtime, manifest.definition_id)
}

fn manifest_parse_fungi_home(args: &CommonArgs) -> std::path::PathBuf {
    let fungi_home = args.fungi_dir();
    if fungi_home.is_absolute() {
        fungi_home
    } else {
        match std::env::current_dir() {
            Ok(current_dir) => current_dir.join(fungi_home),
            Err(_) => fungi_home,
        }
    }
}

async fn confirm_apply_if_existing(
    client: &mut RpcClient,
    device: Option<&super::shared::ResolvedPeerTarget>,
    created: &CreatedServiceManifest,
    args: &CommonArgs,
    yes: bool,
) {
    let (service_name, new_runtime, new_definition_id) = manifest_apply_identity(created, args);
    let existing = match device {
        Some(device) => list_remote_service_instances(client, &device.peer_id)
            .await
            .into_iter()
            .find(|service| service.name == service_name),
        None => list_local_service_instances(client)
            .await
            .into_iter()
            .find(|service| service.name == service_name),
    };

    let Some(existing) = existing else {
        return;
    };

    reject_definition_id_mismatch(&service_name, &existing, new_definition_id.as_deref());

    if yes {
        print_existing_apply_notice(
            &service_name,
            &existing,
            new_runtime,
            new_definition_id.as_deref(),
        );
        return;
    }

    let proceed = if existing.runtime != new_runtime {
        println!(
            "Service {} will change runtime: {} -> {}.",
            service_name,
            runtime_kind_label(existing.runtime),
            runtime_kind_label(new_runtime)
        );
        println!("App data will be kept; runtime artifacts will be replaced.");
        prompt_yes_no_default("Continue? [Y/n]", true)
    } else {
        match (
            existing.definition_id.as_deref(),
            new_definition_id.as_deref(),
        ) {
            (Some(definition_id), _) => {
                println!(
                    "Service {service_name} already exists with definition id `{definition_id}`."
                );
            }
            (None, Some(definition_id)) => {
                println!(
                    "Service {service_name} already exists; the new manifest declares definition id `{definition_id}`."
                );
            }
            (None, None) => {}
        }
        prompt_yes_no_default(
            &format!(
                "Service {} already exists. Apply new manifest and update it? [Y/n]",
                service_name
            ),
            true,
        )
    };

    if !proceed {
        println!("Cancelled");
        std::process::exit(0);
    }
}

fn reject_definition_id_mismatch(
    service_name: &str,
    existing: &ServiceInstance,
    new_definition_id: Option<&str>,
) {
    match (existing.definition_id.as_deref(), new_definition_id) {
        (Some(existing_id), Some(new_id)) if existing_id != new_id => fatal(format!(
            "Service {service_name} already exists with definition id `{existing_id}`, but the new manifest declares `{new_id}`.\nRefusing to replace one service definition with another. Use a different NAME[@DEVICE] or remove the existing service first."
        )),
        (Some(existing_id), None) => fatal(format!(
            "Service {service_name} already exists with definition id `{existing_id}`, but the new manifest does not declare a definition id.\nUse a matching .fungi.md service file or remove the existing service first."
        )),
        _ => {}
    }
}

fn print_existing_apply_notice(
    service_name: &str,
    existing: &ServiceInstance,
    new_runtime: RuntimeKind,
    new_definition_id: Option<&str>,
) {
    if existing.runtime != new_runtime {
        println!(
            "Service {} will change runtime: {} -> {}.",
            service_name,
            runtime_kind_label(existing.runtime),
            runtime_kind_label(new_runtime)
        );
        println!("App data will be kept; runtime artifacts will be replaced.");
        return;
    }

    match (existing.definition_id.as_deref(), new_definition_id) {
        (Some(definition_id), _) => {
            println!(
                "Service {service_name} already exists with definition id `{definition_id}`; applying update."
            );
        }
        (None, Some(definition_id)) => {
            println!(
                "Service {service_name} already exists; applying manifest with definition id `{definition_id}`."
            );
        }
        (None, None) => {
            println!("Service {service_name} already exists; applying update.");
        }
    }
}

fn runtime_kind_label(runtime: RuntimeKind) -> &'static str {
    match runtime {
        RuntimeKind::Docker => "docker",
        RuntimeKind::Wasmtime => "wasmtime",
        RuntimeKind::External => "external",
    }
}

fn prompt_with_default(label: &str, default: &str) -> String {
    let value = prompt(label);
    if value.trim().is_empty() {
        default.to_string()
    } else {
        value.trim().to_string()
    }
}

fn prompt_yes_no_default(label: &str, default: bool) -> bool {
    let default_value = if default { "y" } else { "n" };
    let value = prompt_with_default(label, default_value);
    match value.trim().to_ascii_lowercase().as_str() {
        "y" | "yes" => true,
        "n" | "no" => false,
        _ => fatal("Please answer y or n"),
    }
}

fn prompt(label: &str) -> String {
    print!("{label}: ");
    let _ = io::stdout().flush();
    let mut value = String::new();
    io::stdin()
        .read_line(&mut value)
        .unwrap_or_else(|error| fatal(format!("Failed to read input: {error}")));
    value
}

fn prompt_required(label: &str) -> String {
    let value = prompt(label);
    let value = value.trim();
    if value.is_empty() {
        fatal(format!("{label} is required"))
    }
    value.to_string()
}

fn yaml_key(value: &str) -> String {
    let value = value.trim();
    if !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
    {
        value.to_string()
    } else {
        yaml_scalar(value)
    }
}

fn yaml_scalar(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn usage_kind_label(kind: ServiceExposeUsageKind) -> &'static str {
    match kind {
        ServiceExposeUsageKind::Web => "web",
        ServiceExposeUsageKind::Ssh => "ssh",
        ServiceExposeUsageKind::Raw => "tcp",
    }
}

fn service_usage_label(usage: Option<&fungi_daemon::ServiceExposeUsage>) -> &'static str {
    usage
        .map(|usage| usage_kind_label(usage.kind))
        .unwrap_or("tcp")
}

fn local_service_usage_label(service: &ServiceInstance) -> String {
    let mut labels = service
        .ports
        .iter()
        .map(|port| entry_usage_label(port.name.as_deref()))
        .collect::<Vec<_>>();
    labels.sort_unstable();
    labels.dedup();

    match labels.as_slice() {
        [] => "-".to_string(),
        [label] => (*label).to_string(),
        labels if labels.contains(&"web") && labels.len() == 1 => "web".to_string(),
        _ => "mixed".to_string(),
    }
}

fn access_usage_label(access: &ServiceAccess) -> String {
    let mut labels = access
        .endpoints
        .iter()
        .map(|endpoint| entry_usage_label(Some(endpoint.name.as_str())))
        .collect::<Vec<_>>();
    labels.sort_unstable();
    labels.dedup();

    match labels.as_slice() {
        [] => "-".to_string(),
        [label] => (*label).to_string(),
        _ => "mixed".to_string(),
    }
}

fn entry_usage_label(name: Option<&str>) -> &'static str {
    match name.map(|value| value.trim().to_ascii_lowercase()) {
        Some(value) if matches!(value.as_str(), "web" | "http" | "https") => "web",
        Some(value) if value == "ssh" => "ssh",
        _ => "tcp",
    }
}

pub(crate) fn print_service_instance(resp: ServiceInstanceResponse, verbose: bool) {
    match serde_json::from_str::<ServiceInstance>(&resp.instance_json) {
        Ok(instance) => print_service_instance_value(instance, verbose),
        Err(error) => fatal(format!("Failed to decode service instance: {error}")),
    }
}

pub(crate) fn print_service_instances(resp: ListServicesResponse, verbose: bool) {
    print_service_instances_value(decode_service_instances(resp), verbose)
}

fn decode_service_instances(resp: ListServicesResponse) -> Vec<ServiceInstance> {
    match serde_json::from_str::<Vec<ServiceInstance>>(&resp.services_json) {
        Ok(services) => services,
        Err(error) => fatal(format!("Failed to decode service list: {error}")),
    }
}

fn print_service_instance_value(instance: ServiceInstance, verbose: bool) {
    let pretty = if verbose {
        serde_json::to_string_pretty(&LocalServiceInspectVerboseView::from(instance))
    } else {
        serde_json::to_string_pretty(&LocalServiceInspectView::from(instance))
    };
    match pretty {
        Ok(pretty) => println!("{}", pretty),
        Err(error) => fatal(format!("Failed to format service instance: {error}")),
    }
}

fn decode_service_instance(resp: ServiceInstanceResponse) -> ServiceInstance {
    match serde_json::from_str::<ServiceInstance>(&resp.instance_json) {
        Ok(instance) => instance,
        Err(error) => fatal(format!("Failed to decode service instance: {error}")),
    }
}

fn print_service_instances_value(services: Vec<ServiceInstance>, verbose: bool) {
    let pretty = if verbose {
        let views = services
            .into_iter()
            .map(LocalServiceListVerboseEntry::from)
            .collect::<Vec<_>>();
        serde_json::to_string_pretty(&views)
    } else {
        let views = services
            .into_iter()
            .map(LocalServiceListEntry::from)
            .collect::<Vec<_>>();
        serde_json::to_string_pretty(&views)
    };
    match pretty {
        Ok(pretty) => println!("{}", pretty),
        Err(error) => fatal(format!("Failed to format service list: {error}")),
    }
}

#[derive(Debug, Clone)]
struct ServiceOverviewRow {
    reference: String,
    device: String,
    kind: String,
    usage: String,
    state: String,
    entries: Vec<String>,
    note: Option<String>,
}

impl ServiceOverviewRow {
    fn from_local(service: ServiceInstance, verbose: bool) -> Self {
        let usage = local_service_usage_label(&service);
        let entries = service
            .ports
            .iter()
            .map(|port| {
                let name = port.name.clone().unwrap_or_else(|| "main".to_string());
                if verbose {
                    format!(
                        "{name} 127.0.0.1:{} -> this:{}",
                        port.host_port, port.service_port
                    )
                } else {
                    format!("{name} 127.0.0.1:{}", port.host_port)
                }
            })
            .collect();

        Self {
            reference: service.name.clone(),
            device: "this".to_string(),
            kind: "local".to_string(),
            usage,
            state: service.status.state,
            entries,
            note: None,
        }
    }

    fn from_cached_access(access: ServiceAccess, device: &DeviceInfo, verbose: bool) -> Self {
        let device_name = device_display_name(device);
        let entries = access
            .endpoints
            .iter()
            .map(|endpoint| {
                let remote_port = format_remote_port(endpoint.remote_port);
                if verbose {
                    format!(
                        "{} {}:{} -> {}:{}",
                        endpoint.name,
                        endpoint.local_host,
                        endpoint.local_port,
                        device_name,
                        remote_port
                    )
                } else {
                    format!(
                        "{} {}:{} -> {}:{}",
                        endpoint.name,
                        endpoint.local_host,
                        endpoint.local_port,
                        device_name,
                        remote_port
                    )
                }
            })
            .collect();

        Self {
            reference: format!("{}@{}", access.service_name, device_name),
            device: device_name,
            kind: "remote".to_string(),
            usage: access_usage_label(&access),
            state: "connected".to_string(),
            entries,
            note: None,
        }
    }

    fn from_remote_service(
        service: RemoteService,
        device: &DeviceInfo,
        attached: &[ServiceAccess],
        verbose: bool,
        cached: bool,
    ) -> Self {
        let device_name = device_display_name(device);
        let attached_access = attached
            .iter()
            .find(|access| access.service_name == service.service_name);
        let entries = match attached_access {
            Some(access) => access
                .endpoints
                .iter()
                .map(|endpoint| {
                    let remote_port = format_remote_port(endpoint.remote_port);
                    if verbose {
                        format!(
                            "{} {}:{} -> {}:{}",
                            endpoint.name,
                            endpoint.local_host,
                            endpoint.local_port,
                            device_name,
                            remote_port
                        )
                    } else {
                        format!(
                            "{} {}:{} -> {}:{}",
                            endpoint.name,
                            endpoint.local_host,
                            endpoint.local_port,
                            device_name,
                            remote_port
                        )
                    }
                })
                .collect(),
            None => service
                .endpoints
                .iter()
                .map(|endpoint| {
                    format!(
                        "{} remote:{}:{}",
                        endpoint.name, device_name, endpoint.service_port
                    )
                })
                .collect(),
        };

        Self {
            reference: format!("{}@{}", service.service_name, device_name),
            device: device_name,
            kind: "remote".to_string(),
            usage: service_usage_label(service.usage.as_ref()).to_string(),
            state: if cached {
                "cached".to_string()
            } else {
                service.status.state
            },
            entries,
            note: attached_access.map(|_| "attached".to_string()),
        }
    }

    fn from_remote_managed_service(
        service: ServiceInstance,
        device: &DeviceInfo,
        attached: &[ServiceAccess],
        verbose: bool,
    ) -> Self {
        let device_name = device_display_name(device);
        let attached_access = attached
            .iter()
            .find(|access| access.service_name == service.name);
        let entries = match attached_access {
            Some(access) => access
                .endpoints
                .iter()
                .map(|endpoint| {
                    let remote_port = format_remote_port(endpoint.remote_port);
                    if verbose {
                        format!(
                            "{} {}:{} -> {}:{}",
                            endpoint.name,
                            endpoint.local_host,
                            endpoint.local_port,
                            device_name,
                            remote_port
                        )
                    } else {
                        format!(
                            "{} {}:{} -> {}:{}",
                            endpoint.name,
                            endpoint.local_host,
                            endpoint.local_port,
                            device_name,
                            remote_port
                        )
                    }
                })
                .collect(),
            None => service
                .ports
                .iter()
                .map(|port| {
                    let name = port.name.clone().unwrap_or_else(|| "main".to_string());
                    format!("{name} remote:{}:{}", device_name, port.service_port)
                })
                .collect(),
        };

        Self {
            reference: format!("{}@{}", service.name, device_name),
            device: device_name,
            kind: "remote".to_string(),
            usage: local_service_usage_label(&service),
            state: service.status.state,
            entries,
            note: attached_access.map(|_| "attached".to_string()),
        }
    }

    fn remote_unavailable(device: &DeviceInfo, error: String) -> Self {
        let device_name = device_display_name(device);
        Self {
            reference: format!("@{device_name}"),
            device: device_name,
            kind: "remote".to_string(),
            usage: "-".to_string(),
            state: "unavailable".to_string(),
            entries: Vec::new(),
            note: Some(error),
        }
    }
}

fn print_service_overview_rows(rows: &[ServiceOverviewRow]) {
    if rows.is_empty() {
        println!("No services found");
        return;
    }

    let ref_width = rows
        .iter()
        .map(|row| row.reference.len())
        .max()
        .unwrap_or("SERVICE".len())
        .max("SERVICE".len());
    let device_width = rows
        .iter()
        .map(|row| row.device.len())
        .max()
        .unwrap_or("DEVICE".len())
        .max("DEVICE".len());
    let kind_width = rows
        .iter()
        .map(|row| row.kind.len())
        .max()
        .unwrap_or("KIND".len())
        .max("KIND".len());
    let usage_width = rows
        .iter()
        .map(|row| row.usage.len())
        .max()
        .unwrap_or("TYPE".len())
        .max("TYPE".len());
    let state_width = rows
        .iter()
        .map(|row| row.state.len())
        .max()
        .unwrap_or("STATE".len())
        .max("STATE".len());

    println!(
        "{:<ref_width$}  {:<device_width$}  {:<kind_width$}  {:<usage_width$}  {:<state_width$}  ACCESS",
        "SERVICE", "DEVICE", "KIND", "TYPE", "STATE"
    );
    for row in rows {
        let entries = if row.entries.is_empty() {
            "-".to_string()
        } else {
            row.entries.join(",")
        };
        let suffix = row
            .note
            .as_ref()
            .map(|note| format!("  {note}"))
            .unwrap_or_default();
        println!(
            "{:<ref_width$}  {:<device_width$}  {:<kind_width$}  {:<usage_width$}  {:<state_width$}  {}{}",
            row.reference, row.device, row.kind, row.usage, row.state, entries, suffix
        );
    }
}

fn device_display_name(device: &DeviceInfo) -> String {
    if !device.name.trim().is_empty() {
        device.name.clone()
    } else if !device.hostname.trim().is_empty() {
        device.hostname.clone()
    } else {
        device.peer_id.clone()
    }
}

fn resolved_device_display_name(device: &super::shared::ResolvedPeerTarget) -> String {
    device
        .name
        .as_ref()
        .filter(|name| !name.trim().is_empty())
        .cloned()
        .or_else(|| {
            device
                .hostname
                .as_ref()
                .filter(|hostname| !hostname.trim().is_empty())
                .cloned()
        })
        .unwrap_or_else(|| device.peer_id.clone())
}

fn format_remote_port(port: u16) -> String {
    if port == 0 {
        "?".to_string()
    } else {
        port.to_string()
    }
}

#[derive(Debug, Serialize)]
struct LocalServiceListEntry {
    service_name: String,
    state: String,
    running: bool,
    entries: Vec<ServiceEntryView>,
}

#[derive(Debug, Serialize)]
struct LocalServiceListVerboseEntry {
    service_name: String,
    runtime: RuntimeKind,
    state: String,
    running: bool,
    local_endpoints: Vec<LocalServiceEndpointVerboseView>,
}

#[derive(Debug, Serialize)]
struct LocalServiceInspectView {
    name: String,
    state: String,
    running: bool,
    entries: Vec<ServiceEntryView>,
    published_entries: Vec<ServiceEntryView>,
}

#[derive(Debug, Serialize)]
struct LocalServiceInspectVerboseView {
    id: String,
    name: String,
    runtime: RuntimeKind,
    source: String,
    labels: std::collections::BTreeMap<String, String>,
    state: String,
    running: bool,
    local_endpoints: Vec<LocalServiceEndpointVerboseView>,
    published_endpoints: Vec<PublishedEndpointVerboseView>,
}

#[derive(Debug, Serialize)]
struct ServiceEntryView {
    name: Option<String>,
}

#[derive(Debug, Serialize)]
struct LocalServiceEndpointVerboseView {
    name: Option<String>,
    protocol: String,
    local_host: String,
    local_port: u16,
    service_port: u16,
}

#[derive(Debug, Serialize)]
struct PublishedEndpointVerboseView {
    name: String,
    protocol: String,
    local_host: String,
    local_port: u16,
    service_port: u16,
}

impl From<ServiceInstance> for LocalServiceListEntry {
    fn from(instance: ServiceInstance) -> Self {
        let entries = local_entry_views(&instance);
        Self {
            service_name: instance.name,
            state: instance.status.state,
            running: instance.status.running,
            entries,
        }
    }
}

impl From<ServiceInstance> for LocalServiceListVerboseEntry {
    fn from(instance: ServiceInstance) -> Self {
        let local_endpoints = local_endpoint_verbose_views(&instance);
        Self {
            service_name: instance.name,
            runtime: instance.runtime,
            state: instance.status.state,
            running: instance.status.running,
            local_endpoints,
        }
    }
}

impl From<ServiceInstance> for LocalServiceInspectView {
    fn from(instance: ServiceInstance) -> Self {
        let entries = local_entry_views(&instance);
        Self {
            name: instance.name,
            state: instance.status.state,
            running: instance.status.running,
            entries,
            published_entries: instance
                .exposed_endpoints
                .into_iter()
                .map(|endpoint| ServiceEntryView {
                    name: Some(endpoint.name),
                })
                .collect(),
        }
    }
}

impl From<ServiceInstance> for LocalServiceInspectVerboseView {
    fn from(instance: ServiceInstance) -> Self {
        let local_endpoints = local_endpoint_verbose_views(&instance);
        Self {
            id: instance.id,
            name: instance.name,
            runtime: instance.runtime,
            source: instance.source,
            labels: instance.labels,
            state: instance.status.state,
            running: instance.status.running,
            local_endpoints,
            published_endpoints: instance
                .exposed_endpoints
                .into_iter()
                .map(|endpoint| PublishedEndpointVerboseView {
                    name: endpoint.name,
                    protocol: endpoint.protocol,
                    local_host: "127.0.0.1".to_string(),
                    local_port: endpoint.host_port,
                    service_port: endpoint.service_port,
                })
                .collect(),
        }
    }
}

fn local_entry_views(instance: &ServiceInstance) -> Vec<ServiceEntryView> {
    instance
        .ports
        .iter()
        .map(|port| ServiceEntryView {
            name: port.name.clone(),
        })
        .collect()
}

fn local_endpoint_verbose_views(
    instance: &ServiceInstance,
) -> Vec<LocalServiceEndpointVerboseView> {
    instance
        .ports
        .iter()
        .map(|port| LocalServiceEndpointVerboseView {
            name: port.name.clone(),
            protocol: local_port_protocol_name(port.protocol).to_string(),
            local_host: "127.0.0.1".to_string(),
            local_port: port.host_port,
            service_port: port.service_port,
        })
        .collect()
}

fn local_port_protocol_name(protocol: ServicePortProtocol) -> &'static str {
    match protocol {
        ServicePortProtocol::Tcp => "tcp",
        ServicePortProtocol::Udp => "udp",
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use fungi_daemon::{
        ServiceAccessEndpoint, ServiceExposeTransport, ServiceExposeTransportKind,
        ServiceExposeUsage, ServicePort, ServicePortAllocation, ServiceStatus,
    };

    use super::*;

    #[test]
    fn select_access_endpoint_prefers_requested_entry() {
        let access = service_access(vec![
            access_endpoint("web", 28080),
            access_endpoint("admin", 28081),
        ]);

        let endpoint = select_access_endpoint(&access, Some("admin")).unwrap();

        assert_eq!(endpoint.name, "admin");
        assert_eq!(endpoint.local_port, 28081);
    }

    #[test]
    fn select_access_endpoint_defaults_to_web_then_main() {
        let access = service_access(vec![
            access_endpoint("main", 28080),
            access_endpoint("web", 28081),
        ]);

        let endpoint = select_access_endpoint(&access, None).unwrap();

        assert_eq!(endpoint.name, "web");
        assert_eq!(endpoint.local_port, 28081);
    }

    #[test]
    fn build_web_url_uses_selected_entry_and_usage_path() {
        let access = service_access(vec![
            access_endpoint("web", 28080),
            access_endpoint("admin", 28081),
        ]);
        let service = remote_web_service("/dashboard");

        let url = build_web_url(&service, &access, Some("admin")).unwrap();

        assert_eq!(url, "http://127.0.0.1:28081/dashboard");
    }

    #[test]
    fn select_local_port_defaults_to_web() {
        let instance = service_instance(vec![
            service_port("main", 28080),
            service_port("web", 28081),
        ]);

        let port = select_local_port(&instance, None).unwrap();

        assert_eq!(port.name.as_deref(), Some("web"));
        assert_eq!(port.host_port, 28081);
    }

    #[test]
    fn build_local_web_url_defaults_to_http_named_port() {
        let instance = service_instance(vec![
            service_port("api", 28080),
            service_port("http", 28081),
        ]);

        let url = build_local_web_url(&instance, None).unwrap();

        assert_eq!(url, "http://127.0.0.1:28081");
    }

    #[test]
    fn build_local_web_url_rejects_non_web_default() {
        let instance =
            service_instance(vec![service_port("ssh", 28022), service_port("api", 28080)]);

        let url = build_local_web_url(&instance, None);

        assert!(url.is_none());
    }

    #[test]
    fn build_local_web_url_allows_explicit_entry() {
        let instance = service_instance(vec![service_port("admin", 28081)]);

        let url = build_local_web_url(&instance, Some("admin")).unwrap();

        assert_eq!(url, "http://127.0.0.1:28081");
    }

    #[test]
    fn build_local_web_url_rejects_explicit_tcp_entry() {
        let instance = service_instance(vec![service_port("ssh", 28022)]);

        let url = build_local_web_url(&instance, Some("ssh"));

        assert!(url.is_none());
    }

    #[test]
    fn default_service_list_view_hides_local_ports() {
        let instance =
            service_instance(vec![service_port("web", 28080), service_port("api", 28081)]);

        let view = LocalServiceListEntry::from(instance);
        let json = serde_json::to_value(view).unwrap();
        let text = serde_json::to_string(&json).unwrap();

        assert_eq!(
            json["entries"],
            serde_json::json!([
                { "name": "web" },
                { "name": "api" }
            ])
        );
        assert!(json.get("local_endpoints").is_none());
        assert!(!text.contains("127.0.0.1"));
        assert!(!text.contains("28080"));
        assert!(!text.contains("28081"));
    }

    #[test]
    fn default_service_inspect_view_hides_local_ports() {
        let mut instance = service_instance(vec![service_port("web", 28080)]);
        instance.exposed_endpoints = vec![fungi_daemon::ServiceExposeEndpointBinding {
            name: "web".to_string(),
            protocol: "/fungi/service/demo/web/0.2.0".to_string(),
            host_port: 28080,
            service_port: 80,
        }];

        let view = LocalServiceInspectView::from(instance);
        let json = serde_json::to_value(view).unwrap();
        let text = serde_json::to_string(&json).unwrap();

        assert_eq!(json["entries"], serde_json::json!([{ "name": "web" }]));
        assert_eq!(
            json["published_entries"],
            serde_json::json!([{ "name": "web" }])
        );
        assert!(json.get("local_endpoints").is_none());
        assert!(json.get("published_endpoints").is_none());
        assert!(!text.contains("127.0.0.1"));
        assert!(!text.contains("28080"));
    }

    #[test]
    fn remote_managed_overview_row_keeps_stopped_state() {
        let mut instance = service_instance(vec![service_port("web", 28080)]);
        instance.status = ServiceStatus {
            state: "exited".to_string(),
            running: false,
        };
        let device = device_info("nas");

        let row = ServiceOverviewRow::from_remote_managed_service(instance, &device, &[], false);

        assert_eq!(row.reference, "demo@nas");
        assert_eq!(row.device, "nas");
        assert_eq!(row.kind, "remote");
        assert_eq!(row.usage, "web");
        assert_eq!(row.state, "exited");
        assert_eq!(row.entries, vec!["web remote:nas:80"]);
    }

    #[test]
    fn remove_fallback_only_handles_reachability_errors() {
        let unreachable = tonic::Status::internal(
            "Failed to remove remote service: Failed to open service-control stream to peer abc: No connections available to peer abc",
        );
        let execution_failed = tonic::Status::internal(
            "Failed to remove remote service: execution_failed: container is starting",
        );

        assert!(is_remote_device_reachability_error(&unreachable));
        assert!(!is_remote_device_reachability_error(&execution_failed));
    }

    #[test]
    fn parse_dynamic_thing_target_supports_device_and_entry() {
        let target = parse_dynamic_thing_target("filebrowser@nas/admin".to_string()).unwrap();

        assert_eq!(target.name, "filebrowser");
        assert!(matches!(target.device, Some(DeviceInput::Name(name)) if name == "nas"));
        assert_eq!(target.entry.as_deref(), Some("admin"));
    }

    #[test]
    fn parse_dynamic_thing_invocation_keeps_tool_args() {
        let invocation = parse_dynamic_thing_invocation(vec![
            "rg@nas".to_string(),
            "todo".to_string(),
            "/data".to_string(),
        ])
        .unwrap();

        assert_eq!(invocation.target.name, "rg");
        assert!(matches!(invocation.target.device, Some(DeviceInput::Name(name)) if name == "nas"));
        assert_eq!(invocation.args, vec!["todo", "/data"]);
    }

    #[test]
    fn parse_dynamic_thing_target_rejects_empty_device() {
        let result = parse_dynamic_thing_target("filebrowser@".to_string());

        assert!(result.is_err());
    }

    #[test]
    fn apply_manifest_instance_name_sets_fungi_service_name() {
        let mut created = CreatedServiceManifest {
            manifest_yaml: r#"---
fungi: service/v1
id: webdav
publish:
  ssh:
    tcp:
      port: 22
    client:
      kind: ssh
---

# WebDAV
"#
            .to_string(),
            manifest_base_dir: String::new(),
            start_now: false,
        };

        apply_manifest_instance_name(&mut created, "documents");

        let name = fungi_daemon::peek_service_manifest_name(&created.manifest_yaml).unwrap();
        assert_eq!(name, "documents");
    }

    #[test]
    fn manifest_parse_fungi_home_absolutizes_relative_fungi_dir() {
        let current_dir = std::env::current_dir().unwrap();
        let args = CommonArgs {
            fungi_dir: Some("target/tmp_a".to_string()),
            dynamic_device: None,
            #[cfg(target_os = "android")]
            default_device_name: String::new(),
        };

        assert_eq!(
            manifest_parse_fungi_home(&args),
            current_dir.join("target/tmp_a")
        );
    }

    fn service_access(endpoints: Vec<ServiceAccessEndpoint>) -> ServiceAccess {
        ServiceAccess {
            peer_id: "peer".to_string(),
            service_name: "demo".to_string(),
            endpoints,
        }
    }

    fn access_endpoint(name: &str, local_port: u16) -> ServiceAccessEndpoint {
        ServiceAccessEndpoint {
            name: name.to_string(),
            protocol: format!("/fungi/service/demo/{name}/0.2.0"),
            local_host: "127.0.0.1".to_string(),
            local_port,
            remote_port: local_port,
        }
    }

    fn remote_web_service(path: &str) -> RemoteService {
        RemoteService {
            service_name: "demo".to_string(),
            runtime: RuntimeKind::Docker,
            transport: ServiceExposeTransport {
                kind: ServiceExposeTransportKind::Tcp,
            },
            usage: Some(ServiceExposeUsage {
                kind: ServiceExposeUsageKind::Web,
                path: Some(path.to_string()),
            }),
            icon_url: None,
            catalog_id: None,
            endpoints: Vec::new(),
            status: ServiceStatus {
                state: "running".to_string(),
                running: true,
            },
        }
    }

    fn service_instance(ports: Vec<ServicePort>) -> ServiceInstance {
        ServiceInstance {
            id: "docker:demo".to_string(),
            runtime: RuntimeKind::Docker,
            name: "demo".to_string(),
            definition_id: None,
            source: "demo:latest".to_string(),
            labels: BTreeMap::new(),
            ports,
            exposed_endpoints: Vec::new(),
            status: ServiceStatus {
                state: "running".to_string(),
                running: true,
            },
        }
    }

    fn device_info(name: &str) -> DeviceInfo {
        DeviceInfo {
            peer_id: "peer".to_string(),
            name: name.to_string(),
            hostname: String::new(),
            os: String::new(),
            public_ip: String::new(),
            private_ips: Vec::new(),
            created_at: 0,
            last_connected: 0,
            version: String::new(),
            multiaddrs: Vec::new(),
        }
    }

    fn service_port(name: &str, host_port: u16) -> ServicePort {
        ServicePort {
            name: Some(name.to_string()),
            host_port,
            host_port_allocation: ServicePortAllocation::Auto,
            service_port: 80,
            protocol: ServicePortProtocol::Tcp,
        }
    }
}
