use clap::{CommandFactory, Parser};
use fungi::commands::{
    Commands, FungiArgs,
    fungi_control::{
        DeviceAddressCommands, DeviceCommands, DeviceInput, ServiceArgs, ServiceCommands,
    },
};

#[test]
fn parses_service_add_with_device() {
    let args = FungiArgs::try_parse_from([
        "fungi",
        "service",
        "--device",
        "laptop",
        "add",
        "demo.service.yaml",
    ])
    .unwrap();

    let Commands::Service(ServiceArgs {
        device,
        command: Some(ServiceCommands::Add { manifest }),
        ..
    }) = args.command
    else {
        panic!("expected service add command");
    };

    assert_eq!(manifest.as_deref(), Some("demo.service.yaml"));
    assert!(matches!(device.device, Some(DeviceInput::Name(name)) if name == "laptop"));
}

#[test]
fn parses_interactive_service_add() {
    let args =
        FungiArgs::try_parse_from(["fungi", "service", "--device", "laptop", "add"]).unwrap();

    let Commands::Service(ServiceArgs {
        device,
        command: Some(ServiceCommands::Add { manifest }),
        ..
    }) = args.command
    else {
        panic!("expected service add command");
    };

    assert!(manifest.is_none());
    assert!(matches!(device.device, Some(DeviceInput::Name(name)) if name == "laptop"));
}

#[test]
fn parses_migrate_command() {
    let args = FungiArgs::try_parse_from(["fungi", "migrate"]).unwrap();

    let Commands::Migrate(_) = args.command else {
        panic!("expected migrate command");
    };
}

#[test]
fn parses_service_add_reference_for_interactive_creator() {
    let args = FungiArgs::try_parse_from(["fungi", "service", "add", "ssh@nas"]).unwrap();

    let Commands::Service(ServiceArgs {
        device,
        command: Some(ServiceCommands::Add { manifest }),
        ..
    }) = args.command
    else {
        panic!("expected service add command");
    };

    assert!(device.device.is_none());
    assert_eq!(manifest.as_deref(), Some("ssh@nas"));
}

#[test]
fn parses_service_open_with_named_entry_and_device() {
    let args = FungiArgs::try_parse_from([
        "fungi",
        "service",
        "--device",
        "laptop",
        "open",
        "filebrowser",
        "web",
    ])
    .unwrap();

    let Commands::Service(ServiceArgs {
        device,
        command: Some(ServiceCommands::Open { service, entry }),
        ..
    }) = args.command
    else {
        panic!("expected service open command");
    };

    assert_eq!(service, "filebrowser");
    assert_eq!(entry.as_deref(), Some("web"));
    assert!(matches!(device.device, Some(DeviceInput::Name(name)) if name == "laptop"));
}

#[test]
fn parses_service_connect_with_device() {
    let args = FungiArgs::try_parse_from([
        "fungi", "service", "--device", "home", "connect", "home-ssh",
    ])
    .unwrap();

    let Commands::Service(ServiceArgs {
        device,
        command:
            Some(ServiceCommands::Connect {
                service,
                entry,
                local_port,
            }),
        ..
    }) = args.command
    else {
        panic!("expected service connect command");
    };

    assert_eq!(service, "home-ssh");
    assert!(entry.is_none());
    assert!(local_port.is_none());
    assert!(matches!(device.device, Some(DeviceInput::Name(name)) if name == "home"));
}

#[test]
fn parses_service_connect_with_fixed_local_port() {
    let args = FungiArgs::try_parse_from([
        "fungi",
        "service",
        "--device",
        "home",
        "connect",
        "home-ssh",
        "ssh",
        "--local-port",
        "2222",
    ])
    .unwrap();

    let Commands::Service(ServiceArgs {
        command:
            Some(ServiceCommands::Connect {
                service,
                entry,
                local_port,
            }),
        ..
    }) = args.command
    else {
        panic!("expected service connect command");
    };

    assert_eq!(service, "home-ssh");
    assert_eq!(entry.as_deref(), Some("ssh"));
    assert_eq!(local_port, Some(2222));
}

#[test]
fn parses_service_disconnect_reference() {
    let args =
        FungiArgs::try_parse_from(["fungi", "service", "disconnect", "home-ssh@nas"]).unwrap();

    let Commands::Service(ServiceArgs {
        command: Some(ServiceCommands::Disconnect { service }),
        ..
    }) = args.command
    else {
        panic!("expected service disconnect command");
    };

    assert_eq!(service, "home-ssh@nas");
}

#[test]
fn parses_service_set_local_port() {
    let args =
        FungiArgs::try_parse_from(["fungi", "service", "set", "home-ssh@nas", "local.port=2222"])
            .unwrap();

    let Commands::Service(ServiceArgs {
        command: Some(ServiceCommands::Set { service, setting }),
        ..
    }) = args.command
    else {
        panic!("expected service set command");
    };

    assert_eq!(service, "home-ssh@nas");
    assert_eq!(setting, "local.port=2222");
}

#[test]
fn parses_service_list_with_device() {
    let args = FungiArgs::try_parse_from(["fungi", "service", "--device", "home", "list"]).unwrap();

    let Commands::Service(ServiceArgs {
        device,
        command: Some(ServiceCommands::List { verbose, refresh }),
        ..
    }) = args.command
    else {
        panic!("expected service list command");
    };

    assert!(!verbose);
    assert!(!refresh);
    assert!(matches!(device.device, Some(DeviceInput::Name(name)) if name == "home"));
}

#[test]
fn parses_service_start_with_device() {
    let args = FungiArgs::try_parse_from([
        "fungi",
        "service",
        "--device",
        "home",
        "start",
        "filebrowser",
    ])
    .unwrap();

    let Commands::Service(ServiceArgs {
        device,
        command: Some(ServiceCommands::Start { name }),
        ..
    }) = args.command
    else {
        panic!("expected service start command");
    };

    assert_eq!(name, "filebrowser");
    assert!(matches!(device.device, Some(DeviceInput::Name(name)) if name == "home"));
}

#[test]
fn parses_service_start_reference() {
    let args =
        FungiArgs::try_parse_from(["fungi", "service", "start", "filebrowser@home"]).unwrap();

    let Commands::Service(ServiceArgs {
        device,
        command: Some(ServiceCommands::Start { name }),
        ..
    }) = args.command
    else {
        panic!("expected service start command");
    };

    assert!(device.device.is_none());
    assert_eq!(name, "filebrowser@home");
}

#[test]
fn parses_service_stop_with_device() {
    let args = FungiArgs::try_parse_from([
        "fungi",
        "service",
        "--device",
        "home",
        "stop",
        "filebrowser",
    ])
    .unwrap();

    let Commands::Service(ServiceArgs {
        device,
        command: Some(ServiceCommands::Stop { name }),
        ..
    }) = args.command
    else {
        panic!("expected service stop command");
    };

    assert_eq!(name, "filebrowser");
    assert!(matches!(device.device, Some(DeviceInput::Name(name)) if name == "home"));
}

#[test]
fn parses_service_remove_with_device() {
    let args = FungiArgs::try_parse_from([
        "fungi",
        "service",
        "--device",
        "home",
        "remove",
        "filebrowser",
    ])
    .unwrap();

    let Commands::Service(ServiceArgs {
        device,
        command: Some(ServiceCommands::Remove { name }),
        ..
    }) = args.command
    else {
        panic!("expected service remove command");
    };

    assert_eq!(name, "filebrowser");
    assert!(matches!(device.device, Some(DeviceInput::Name(name)) if name == "home"));
}

#[test]
fn parses_service_inspect_with_device() {
    let args = FungiArgs::try_parse_from([
        "fungi",
        "service",
        "--device",
        "home",
        "inspect",
        "filebrowser",
    ])
    .unwrap();

    let Commands::Service(ServiceArgs {
        device,
        command: Some(ServiceCommands::Inspect { name, verbose }),
        ..
    }) = args.command
    else {
        panic!("expected service inspect command");
    };

    assert_eq!(name, "filebrowser");
    assert!(!verbose);
    assert!(matches!(device.device, Some(DeviceInput::Name(name)) if name == "home"));
}

#[test]
fn parses_service_target_device_names() {
    let by_device =
        FungiArgs::try_parse_from(["fungi", "service", "--device", "home", "list"]).unwrap();
    let Commands::Service(ServiceArgs { device, .. }) = by_device.command else {
        panic!("expected service list command");
    };
    assert!(matches!(device.device, Some(DeviceInput::Name(name)) if name == "home"));

    let by_short = FungiArgs::try_parse_from(["fungi", "service", "-d", "nas", "list"]).unwrap();
    let Commands::Service(ServiceArgs { device, .. }) = by_short.command else {
        panic!("expected service list command");
    };
    assert!(matches!(device.device, Some(DeviceInput::Name(name)) if name == "nas"));
}

#[test]
fn parses_bare_service_as_default_list() {
    let args = FungiArgs::try_parse_from(["fungi", "service"]).unwrap();

    let Commands::Service(ServiceArgs {
        device,
        refresh,
        command,
    }) = args.command
    else {
        panic!("expected service command");
    };

    assert!(device.device.is_none());
    assert!(!refresh);
    assert!(command.is_none());
}

#[test]
fn parses_service_refresh_overview() {
    let args = FungiArgs::try_parse_from(["fungi", "service", "--refresh"]).unwrap();

    let Commands::Service(ServiceArgs {
        refresh, command, ..
    }) = args.command
    else {
        panic!("expected service command");
    };

    assert!(refresh);
    assert!(command.is_none());
}

#[test]
fn parses_service_list_refresh() {
    let args = FungiArgs::try_parse_from(["fungi", "service", "list", "--refresh"]).unwrap();

    let Commands::Service(ServiceArgs {
        command: Some(ServiceCommands::List { refresh, .. }),
        ..
    }) = args.command
    else {
        panic!("expected service list command");
    };

    assert!(refresh);
}

#[test]
fn parses_bare_device_as_default_list() {
    let args = FungiArgs::try_parse_from(["fungi", "device"]).unwrap();

    let Commands::Device(device_args) = args.command else {
        panic!("expected device command");
    };

    assert!(device_args.command.is_none());
}

#[test]
fn parses_device_list_explicitly() {
    let args = FungiArgs::try_parse_from(["fungi", "device", "list"]).unwrap();

    let Commands::Device(device_args) = args.command else {
        panic!("expected device list command");
    };

    assert!(matches!(device_args.command, Some(DeviceCommands::List)));
}

#[test]
fn parses_device_add_with_manual_address() {
    let args = FungiArgs::try_parse_from([
        "fungi",
        "device",
        "add",
        "12D3KooWExample",
        "--name",
        "nas",
        "--addr",
        "/ip4/127.0.0.1/tcp/4001",
    ])
    .unwrap();

    let Commands::Device(device_args) = args.command else {
        panic!("expected device command");
    };
    let Some(DeviceCommands::Add {
        peer_id,
        name,
        addresses,
    }) = device_args.command
    else {
        panic!("expected device add command");
    };

    assert_eq!(peer_id, "12D3KooWExample");
    assert_eq!(name, "nas");
    assert_eq!(addresses, vec!["/ip4/127.0.0.1/tcp/4001"]);
}

#[test]
fn parses_device_address_add() {
    let args = FungiArgs::try_parse_from([
        "fungi",
        "device",
        "address",
        "add",
        "nas",
        "/ip4/127.0.0.1/tcp/4001",
    ])
    .unwrap();

    let Commands::Device(device_args) = args.command else {
        panic!("expected device command");
    };
    let Some(DeviceCommands::Address(DeviceAddressCommands::Add { device, address })) =
        device_args.command
    else {
        panic!("expected device address add command");
    };

    assert_eq!(device, "nas");
    assert_eq!(address, "/ip4/127.0.0.1/tcp/4001");
}

#[test]
fn parses_dynamic_thing_at_device() {
    let args = FungiArgs::try_parse_from(["fungi", "filebrowser@nas"]).unwrap();

    let Commands::Dynamic(tokens) = args.command else {
        panic!("expected dynamic thing command");
    };

    assert!(args.common.dynamic_device.is_none());
    assert_eq!(tokens, vec!["filebrowser@nas"]);
}

#[test]
fn parses_dynamic_thing_with_device_context() {
    let args = FungiArgs::try_parse_from(["fungi", "-d", "nas", "filebrowser"]).unwrap();

    let Commands::Dynamic(tokens) = args.command else {
        panic!("expected dynamic thing command");
    };

    assert!(matches!(
        args.common.dynamic_device,
        Some(DeviceInput::Name(name)) if name == "nas"
    ));
    assert_eq!(tokens, vec!["filebrowser"]);
}

#[test]
fn parses_dynamic_tool_style_args() {
    let args = FungiArgs::try_parse_from(["fungi", "rg@nas", "todo", "/data"]).unwrap();

    let Commands::Dynamic(tokens) = args.command else {
        panic!("expected dynamic thing command");
    };

    assert_eq!(tokens, vec!["rg@nas", "todo", "/data"]);
}

#[test]
fn rejects_service_subcommand_scoped_device() {
    let result =
        FungiArgs::try_parse_from(["fungi", "service", "open", "home-ssh", "--device", "nas"]);

    assert!(result.is_err());
}

#[test]
fn root_help_marks_tunnel_deprecated_and_hides_service_plumbing_commands() {
    let help = FungiArgs::command().render_long_help().to_string();

    assert!(help.contains("  service"));
    assert!(help.contains("  tunnel"));
    assert!(help.contains("Deprecated: manage raw TCP tunneling"));
    assert!(!help.contains("  catalog"));
    assert!(!help.contains("  access"));
    assert!(!help.contains("  peer"));
}

#[test]
fn service_help_hides_pull_shortcut_from_main_path() {
    let mut command = FungiArgs::command();
    let service = command
        .find_subcommand_mut("service")
        .expect("service command exists");
    let help = service.render_long_help().to_string();

    assert!(help.contains("  add"));
    assert!(help.contains("  open"));
    assert!(help.contains("  connect"));
    assert!(!help.contains("  pull"));
}

#[test]
fn service_command_help_prefers_device_language() {
    let mut command = FungiArgs::command();
    let service = command
        .find_subcommand_mut("service")
        .expect("service command exists");
    let help = service.render_long_help().to_string();

    assert!(help.contains("--device <DEVICE>"));
    assert!(help.contains("-d"));
    assert!(!help.contains("--on"));
    assert!(!help.contains("--peer"));
    assert!(!help.contains("Peer ID"));
}
