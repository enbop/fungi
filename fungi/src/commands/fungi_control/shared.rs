use clap::Args;
use fungi_config::{FungiDir, devices::DevicesConfig};
use libp2p::PeerId;
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

use crate::commands::CommonArgs;

const CLI_CONTEXT_FILE: &str = "cli_context.json";

#[derive(Args, Debug, Clone)]
pub struct PeerTargetArg {
    #[arg(short = 'p', long = "peer", help = "Device ID or name")]
    pub peer: Option<PeerInput>,
}

#[derive(Args, Debug, Clone, Default)]
pub struct OptionalPeerTargetArg {
    #[arg(short = 'p', long = "peer", help = "Device ID or name")]
    pub peer: Option<PeerInput>,
}

#[derive(Args, Debug, Clone, Default)]
pub struct OptionalDeviceTargetArg {
    #[arg(
        short = 'd',
        long = "device",
        value_name = "DEVICE",
        help = "Device name"
    )]
    pub device: Option<DeviceInput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeerInput {
    PeerId(PeerId),
    Name(String),
}

pub use PeerInput as DeviceInput;

impl FromStr for PeerInput {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let value = value.trim();
        if value.is_empty() {
            return Err("Device ID or name cannot be empty".to_string());
        }

        if let Ok(peer_id) = value.parse::<PeerId>() {
            return Ok(Self::PeerId(peer_id));
        }

        Ok(Self::Name(value.to_string()))
    }
}

impl fmt::Display for PeerInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PeerId(peer_id) => write!(f, "{peer_id}"),
            Self::Name(name) => write!(f, "{name}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedPeerTarget {
    pub peer_id: String,
    pub name: Option<String>,
    pub hostname: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct CliContext {
    #[serde(default)]
    current_peer_id: Option<String>,
    #[serde(default)]
    current_peer_name: Option<String>,
}

pub fn resolve_required_peer(
    args: &CommonArgs,
    peer: Option<&PeerInput>,
) -> Result<ResolvedPeerTarget, String> {
    if let Some(peer) = peer {
        return resolve_peer_input(args, peer);
    }

    if let Some(peer) = get_current_peer(args)? {
        return Ok(peer);
    }

    Err("No device selected. Use a device name or device ID.".to_string())
}

pub fn resolve_optional_peer(
    args: &CommonArgs,
    peer: Option<&PeerInput>,
) -> Result<Option<ResolvedPeerTarget>, String> {
    match peer {
        Some(peer) => resolve_peer_input(args, peer).map(Some),
        None => Ok(None),
    }
}

pub fn resolve_optional_device(
    args: &CommonArgs,
    device: Option<&DeviceInput>,
) -> Result<Option<ResolvedPeerTarget>, String> {
    resolve_optional_peer(args, device)
}

pub fn get_current_peer(args: &CommonArgs) -> Result<Option<ResolvedPeerTarget>, String> {
    let Some(peer_id) = load_cli_context(args)?.current_peer_id else {
        return Ok(None);
    };
    resolve_peer_value(args, &peer_id).map(Some)
}

pub fn set_current_peer(args: &CommonArgs, peer: &ResolvedPeerTarget) -> Result<(), String> {
    let context = CliContext {
        current_peer_id: Some(peer.peer_id.clone()),
        current_peer_name: peer.name.clone(),
    };
    save_cli_context(args, &context)
}

pub fn clear_current_peer(args: &CommonArgs) -> Result<(), String> {
    let path = cli_context_path(args);
    if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|error| format!("Failed to clear current peer context: {error}"))?;
    }
    Ok(())
}

pub fn resolve_peer_input(
    args: &CommonArgs,
    peer: &PeerInput,
) -> Result<ResolvedPeerTarget, String> {
    match peer {
        PeerInput::PeerId(peer_id) => {
            let device_info = lookup_device_info(args, &peer_id.to_string())?;
            let name = device_info.as_ref().and_then(|entry| entry.name.clone());
            if name.is_none() {
                return Err(format!(
                    "Device ID {} is not saved yet. Add it first with `fungi device add <name> {}`.",
                    peer_id, peer_id
                ));
            }

            Ok(ResolvedPeerTarget {
                peer_id: peer_id.to_string(),
                name,
                hostname: device_info.and_then(|entry| entry.hostname.clone()),
            })
        }
        PeerInput::Name(name) => {
            let devices = DevicesConfig::apply_from_dir(&args.fungi_dir())
                .map_err(|error| format!("Failed to load devices: {error}"))?;
            match devices.get_device_by_name(name) {
                Some(entry) => Ok(ResolvedPeerTarget {
                    peer_id: entry.peer_id.to_string(),
                    name: entry.name.clone(),
                    hostname: entry.hostname.clone(),
                }),
                None => Err(format!(
                    "Unknown device name: {name}. Add or rename the device first."
                )),
            }
        }
    }
}

pub fn resolve_peer_value(args: &CommonArgs, peer: &str) -> Result<ResolvedPeerTarget, String> {
    let peer = peer.parse::<PeerInput>()?;
    resolve_peer_input(args, &peer)
}

fn lookup_device_info(
    args: &CommonArgs,
    peer_id: &str,
) -> Result<Option<fungi_config::devices::DeviceInfo>, String> {
    let devices = DevicesConfig::apply_from_dir(&args.fungi_dir())
        .map_err(|error| format!("Failed to load devices: {error}"))?;
    Ok(devices
        .get_all_devices()
        .iter()
        .find(|entry| entry.peer_id.to_string() == peer_id)
        .cloned())
}

pub fn print_target_peer(peer: &ResolvedPeerTarget) {
    let name = peer.name.as_deref().unwrap_or("<unnamed>");
    match peer.hostname.as_deref() {
        Some(hostname) if !hostname.is_empty() => {
            eprintln!("Target device: {name} ({}) [{hostname}]", peer.peer_id)
        }
        _ => eprintln!("Target device: {name} ({})", peer.peer_id),
    }
}

pub fn print_target_device(device: &ResolvedPeerTarget) {
    let name = device.name.as_deref().unwrap_or("<unnamed>");
    eprintln!("Target device: {name}");
}

fn cli_context_path(args: &CommonArgs) -> std::path::PathBuf {
    args.fungi_dir().join(CLI_CONTEXT_FILE)
}

fn load_cli_context(args: &CommonArgs) -> Result<CliContext, String> {
    let path = cli_context_path(args);
    if !path.exists() {
        return Ok(CliContext::default());
    }

    let raw = std::fs::read_to_string(&path)
        .map_err(|error| format!("Failed to read CLI context: {error}"))?;
    serde_json::from_str(&raw).map_err(|error| format!("Failed to parse CLI context: {error}"))
}

fn save_cli_context(args: &CommonArgs, context: &CliContext) -> Result<(), String> {
    let path = cli_context_path(args);
    let raw = serde_json::to_string_pretty(context)
        .map_err(|error| format!("Failed to encode CLI context: {error}"))?;
    std::fs::write(path, raw).map_err(|error| format!("Failed to save CLI context: {error}"))
}

pub fn parse_address(address: &str) -> Result<(String, u16), String> {
    let parts: Vec<&str> = address.rsplitn(2, ':').collect();
    if parts.len() != 2 {
        return Err(format!(
            "Invalid address format: {}. Expected format: host:port",
            address
        ));
    }

    let port = parts[0]
        .parse::<u16>()
        .map_err(|_| format!("Invalid port number: {}", parts[0]))?;
    let host = parts[1].to_string();

    Ok((host, port))
}

pub fn fatal(message: impl std::fmt::Display) -> ! {
    eprintln!("{message}");
    std::process::exit(1);
}

pub fn fatal_grpc(error: impl std::fmt::Display) -> ! {
    fatal(format!("Error: {error}"))
}

pub fn shorten_peer_id(peer_id: &str) -> String {
    if peer_id.len() <= 18 {
        return peer_id.to_string();
    }
    format!("{}****{}", &peer_id[..8], &peer_id[peer_id.len() - 6..])
}

pub fn simplify_multiaddr_peer_ids(addr: &str) -> String {
    let mut parts: Vec<String> = addr
        .split('/')
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .collect();

    let mut i = 0;
    while i + 1 < parts.len() {
        if parts[i] == "p2p" {
            parts[i + 1] = shorten_peer_id(&parts[i + 1]);
            i += 2;
        } else {
            i += 1;
        }
    }

    format!("/{}", parts.join("/"))
}

pub fn connection_id_sort_key(connection_id: &str) -> u64 {
    if let Ok(value) = connection_id.parse::<u64>() {
        return value;
    }

    let digits: String = connection_id
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect();
    digits.parse::<u64>().unwrap_or(u64::MAX)
}

pub fn summarize_ping_error_message(message: &str, verbose: bool) -> String {
    if verbose {
        return message.to_string();
    }

    let lower = message.to_lowercase();
    if !lower.contains("failed to negotiate transport protocol") {
        return message.to_string();
    }

    let attempts_section = message
        .split_once("[")
        .and_then(|(_, rest)| rest.rsplit_once("]").map(|(inside, _)| inside));

    let Some(raw_attempts) = attempts_section else {
        return "Dial failed (transport negotiation failed, use -v for details)".to_string();
    };

    let attempts = raw_attempts
        .split(")(")
        .map(|part| part.trim_matches(|c| c == '(' || c == ')'))
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();

    if attempts.is_empty() {
        return "Dial failed (transport negotiation failed, use -v for details)".to_string();
    }

    let mut refused = 0usize;
    let mut timed_out = 0usize;
    let mut relay_failed = 0usize;
    let mut other = 0usize;

    for attempt in &attempts {
        let content = attempt.to_lowercase();
        if content.contains("connection refused") {
            refused += 1;
        } else if content.contains("timed out") || content.contains("timeout") {
            timed_out += 1;
        } else if content.contains("relay failed")
            || content.contains("failed to connect to destination")
        {
            relay_failed += 1;
        } else {
            other += 1;
        }
    }

    let mut parts = Vec::new();
    if refused > 0 {
        parts.push(format!("refused={refused}"));
    }
    if timed_out > 0 {
        parts.push(format!("timeout={timed_out}"));
    }
    if relay_failed > 0 {
        parts.push(format!("relay_failed={relay_failed}"));
    }
    if other > 0 {
        parts.push(format!("other={other}"));
    }

    format!(
        "Dial failed (attempts={}, {}, use -v for full details)",
        attempts.len(),
        parts.join(", ")
    )
}

pub fn host_path_risk_note(path: &str) -> Option<String> {
    let path = std::path::Path::new(path);
    if path == std::path::Path::new("/") {
        return Some("CRITICAL: this exposes the entire host filesystem".to_string());
    }

    let home_dir = home::home_dir()?;
    if path == home_dir {
        return Some("HIGH: this exposes the entire current user's home directory".to_string());
    }

    if home_dir.starts_with(path) {
        return Some("HIGH: this path contains the current user's home directory".to_string());
    }

    None
}
