use clap::{CommandFactory, Parser};
use fungi::commands::{
    Commands, FungiArgs,
    fungi_control::{DeviceCommands, DeviceInput, ServiceArgs, ServiceCommands},
};

#[test]
fn parses_service_add_with_on_device() {
    let args = FungiArgs::try_parse_from([
        "fungi",
        "service",
        "--on",
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

    assert_eq!(manifest, "demo.service.yaml");
    assert!(matches!(device.device, Some(DeviceInput::Alias(alias)) if alias == "laptop"));
}

#[test]
fn parses_service_open_with_named_entry_and_on_device() {
    let args = FungiArgs::try_parse_from([
        "fungi",
        "service",
        "--on",
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
    assert!(matches!(device.device, Some(DeviceInput::Alias(alias)) if alias == "laptop"));
}

#[test]
fn parses_service_connect_with_on_device() {
    let args =
        FungiArgs::try_parse_from(["fungi", "service", "--on", "home", "connect", "home-ssh"])
            .unwrap();

    let Commands::Service(ServiceArgs {
        device,
        command: Some(ServiceCommands::Connect { service, entry }),
        ..
    }) = args.command
    else {
        panic!("expected service connect command");
    };

    assert_eq!(service, "home-ssh");
    assert!(entry.is_none());
    assert!(matches!(device.device, Some(DeviceInput::Alias(alias)) if alias == "home"));
}

#[test]
fn parses_service_list_with_on_device() {
    let args = FungiArgs::try_parse_from(["fungi", "service", "--on", "home", "list"]).unwrap();

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
    assert!(matches!(device.device, Some(DeviceInput::Alias(alias)) if alias == "home"));
}

#[test]
fn parses_service_start_with_on_device() {
    let args =
        FungiArgs::try_parse_from(["fungi", "service", "--on", "home", "start", "filebrowser"])
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
    assert!(matches!(device.device, Some(DeviceInput::Alias(alias)) if alias == "home"));
}

#[test]
fn parses_service_stop_with_on_device() {
    let args =
        FungiArgs::try_parse_from(["fungi", "service", "--on", "home", "stop", "filebrowser"])
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
    assert!(matches!(device.device, Some(DeviceInput::Alias(alias)) if alias == "home"));
}

#[test]
fn parses_service_remove_with_on_device() {
    let args =
        FungiArgs::try_parse_from(["fungi", "service", "--on", "home", "remove", "filebrowser"])
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
    assert!(matches!(device.device, Some(DeviceInput::Alias(alias)) if alias == "home"));
}

#[test]
fn parses_service_inspect_with_on_device() {
    let args =
        FungiArgs::try_parse_from(["fungi", "service", "--on", "home", "inspect", "filebrowser"])
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
    assert!(matches!(device.device, Some(DeviceInput::Alias(alias)) if alias == "home"));
}

#[test]
fn parses_service_target_device_aliases() {
    let by_device =
        FungiArgs::try_parse_from(["fungi", "service", "--device", "home", "list"]).unwrap();
    let Commands::Service(ServiceArgs { device, .. }) = by_device.command else {
        panic!("expected service list command");
    };
    assert!(matches!(device.device, Some(DeviceInput::Alias(alias)) if alias == "home"));

    let by_short = FungiArgs::try_parse_from(["fungi", "service", "-d", "nas", "list"]).unwrap();
    let Commands::Service(ServiceArgs { device, .. }) = by_short.command else {
        panic!("expected service list command");
    };
    assert!(matches!(device.device, Some(DeviceInput::Alias(alias)) if alias == "nas"));
}

#[test]
fn parses_hidden_service_peer_compatibility_flag() {
    let args = FungiArgs::try_parse_from(["fungi", "service", "--peer", "home", "list"]).unwrap();

    let Commands::Service(ServiceArgs { device, .. }) = args.command else {
        panic!("expected service list command");
    };

    assert!(matches!(device.device, Some(DeviceInput::Alias(alias)) if alias == "home"));
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
        Some(DeviceInput::Alias(alias)) if alias == "nas"
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
fn rejects_service_subcommand_scoped_on_device() {
    let result = FungiArgs::try_parse_from(["fungi", "service", "open", "home-ssh", "--on", "nas"]);

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
fn service_help_hides_pull_alias_from_main_path() {
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

    assert!(help.contains("--on <DEVICE>"));
    assert!(help.contains("-d"));
    assert!(help.contains("--device"));
    assert!(!help.contains("--peer"));
    assert!(!help.contains("Peer ID"));
}
