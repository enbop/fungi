pub mod fungi_control;
pub mod fungi_daemon;
pub mod fungi_init;
pub mod fungi_migrate;
pub mod fungi_relay;

use std::path::PathBuf;

use clap::{CommandFactory, Parser, Subcommand};
use fungi_config::{FungiDir, default_fungi_dir_name};
use fungi_control::DeviceInput;

/// A platform built for seamless multi-device integration
#[derive(Parser)]
#[command(version, about, long_about = None)]
pub struct FungiArgs {
    #[command(flatten)]
    pub common: CommonArgs,
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Clone, Default, Parser)]
pub struct CommonArgs {
    #[clap(
        short,
        long,
        help = "Path to the Fungi config directory, defaults to the channel-specific directory"
    )]
    pub fungi_dir: Option<String>,

    #[clap(
        short = 'd',
        long = "device",
        value_name = "DEVICE",
        help = "Device context for dynamic thing calls"
    )]
    pub dynamic_device: Option<DeviceInput>,

    #[cfg(target_os = "android")]
    #[clap(
        long,
        default_value = "",
        help = "Set default device info string for this device, only used in Android"
    )]
    pub default_device_name: String,
}

impl FungiDir for CommonArgs {
    fn fungi_dir(&self) -> PathBuf {
        self.fungi_dir
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                home::home_dir()
                    .unwrap_or_else(|| {
                        panic!(
                            "Unable to determine home directory. Please provide --fungi-dir explicitly."
                        )
                    })
                    .join(default_fungi_dir_name())
            })
    }
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize a Fungi configuration, and generate a keypair
    Init(fungi_init::InitArgs),
    /// Migrate an existing Fungi configuration directory to the current schema
    Migrate(fungi_migrate::MigrateArgs),
    /// Start a Fungi daemon or daemon-managed background services
    Daemon(fungi_daemon::DaemonCommandArgs),

    /// Manage relay configuration for the local daemon
    #[command(subcommand)]
    Relay(fungi_control::RelayCommands),

    /// Show daemon information
    #[command(subcommand)]
    Info(fungi_control::InfoCommands),
    /// Manage runtime safety boundary settings
    #[command(subcommand, visible_alias = "sec")]
    Security(fungi_control::SecurityCommands),
    /// Manage services on this device or another device
    #[command(visible_alias = "svc")]
    Service(fungi_control::ServiceArgs),
    /// Browse published remote services
    #[command(subcommand, hide = true)]
    Catalog(fungi_control::CatalogCommands),
    /// Manage local access entries for remote services
    #[command(subcommand, hide = true)]
    Access(fungi_control::AccessCommands),
    /// Query and administer remote peers
    #[command(subcommand, hide = true)]
    Peer(fungi_control::PeerCommands),
    /// Device discovery and saved devices
    Device(fungi_control::DeviceArgs),
    /// Connection observability and diagnostics
    #[command(subcommand, visible_alias = "conn")]
    Connection(fungi_control::ConnectionCommands),
    /// Continuously ping all active connections to a device
    Ping {
        /// Device name to ping
        peer: fungi_control::PeerInput,
        /// Ping interval in milliseconds
        #[arg(long, default_value_t = 2000)]
        interval_ms: u32,
        /// Show detailed output
        #[arg(short, long, default_value_t = false)]
        verbose: bool,
    },
    #[cfg(feature = "wasi")]
    /// [WASI runtime] Run a WebAssembly module (re-exported wasmtime command)
    Run(wasmtime_cli::commands::RunCommand),
    #[cfg(feature = "wasi")]
    /// [WASI runtime] Serve wasi-http requests (re-exported wasmtime command)
    Serve(wasmtime_cli::commands::ServeCommand),
    /// Deprecated: manage raw TCP tunneling; prefer `service open/connect`
    #[command(subcommand, visible_alias = "tn")]
    Tunnel(fungi_control::TunnelCommands),
    /// Deprecated: manage legacy file transfer service; this command will be removed in a future release
    #[command(subcommand, visible_alias = "fs")]
    FtService(fungi_control::FtServiceCommands),
    /// Deprecated: manage legacy file transfer client config and FTP/WebDAV proxies; this command will be removed in a future release
    #[command(subcommand, visible_alias = "fc")]
    FtClient(fungi_control::FtClientCommands),
    /// Invoke a service or tool by name
    #[command(external_subcommand)]
    Dynamic(Vec<String>),
}

pub fn dynamic_builtin_typo_hint_for_tokens(
    tokens: &[String],
    device_context: Option<&DeviceInput>,
) -> Option<(String, String)> {
    if device_context.is_some() || tokens.len() != 1 {
        return None;
    }

    let target = fungi_control::parse_dynamic_thing_target(tokens[0].clone()).ok()?;
    if target.device.is_some() || target.entry.is_some() {
        return None;
    }

    let mut command = FungiArgs::command();
    command.build();
    let mut command = command.allow_external_subcommands(false);
    let err = command
        .try_get_matches_from_mut(["fungi", target.name.as_str()])
        .err()?;
    if err.kind() != clap::error::ErrorKind::InvalidSubcommand {
        return None;
    }

    let suggestion = match err.get(clap::error::ContextKind::SuggestedSubcommand)? {
        clap::error::ContextValue::String(value) => value.clone(),
        clap::error::ContextValue::Strings(values) => values
            .iter()
            .min_by_key(|value| edit_distance(&target.name, value))
            .cloned()?,
        _ => return None,
    };

    Some((target.name, suggestion))
}

fn edit_distance(left: &str, right: &str) -> usize {
    let left = left.as_bytes();
    let right = right.as_bytes();
    let mut previous: Vec<usize> = (0..=right.len()).collect();
    let mut current = vec![0; right.len() + 1];

    for (left_index, left_byte) in left.iter().enumerate() {
        current[0] = left_index + 1;
        for (right_index, right_byte) in right.iter().enumerate() {
            let substitution = previous[right_index] + usize::from(left_byte != right_byte);
            let insertion = current[right_index] + 1;
            let deletion = previous[right_index + 1] + 1;
            current[right_index + 1] = substitution.min(insertion).min(deletion);
        }
        std::mem::swap(&mut previous, &mut current);
    }

    previous[right.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dynamic_builtin_typo_hint_uses_clap_subcommand_suggestions() {
        assert_eq!(
            dynamic_builtin_typo_hint_for_tokens(&["devices".to_string()], None),
            Some(("devices".to_string(), "device".to_string()))
        );
        assert_eq!(
            dynamic_builtin_typo_hint_for_tokens(&["devic".to_string()], None),
            Some(("devic".to_string(), "device".to_string()))
        );
    }

    #[test]
    fn dynamic_builtin_typo_hint_only_checks_unscoped_single_dynamic_targets() {
        assert_eq!(
            dynamic_builtin_typo_hint_for_tokens(
                &["devices".to_string(), "extra".to_string()],
                None
            ),
            None
        );
        assert_eq!(
            dynamic_builtin_typo_hint_for_tokens(&["devices@nas".to_string()], None),
            None
        );
        assert_eq!(
            dynamic_builtin_typo_hint_for_tokens(
                &["devices".to_string()],
                Some(&DeviceInput::Name("nas".to_string()))
            ),
            None
        );
        assert_eq!(
            dynamic_builtin_typo_hint_for_tokens(&["filebrowser".to_string()], None),
            None
        );
    }
}
