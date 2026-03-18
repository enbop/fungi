use clap::Args;
use fungi_config::{FungiDir, address_book::AddressBookConfig};
use libp2p::PeerId;
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

use crate::commands::CommonArgs;

const CLI_CONTEXT_FILE: &str = "cli_context.json";

#[derive(Args, Debug, Clone)]
pub struct PeerTargetArg {
    #[arg(short = 'p', long = "peer", help = "Peer ID or alias")]
    pub peer: Option<PeerInput>,
}

#[derive(Args, Debug, Clone, Default)]
pub struct OptionalPeerTargetArg {
    #[arg(short = 'p', long = "peer", help = "Peer ID or alias")]
    pub peer: Option<PeerInput>,
}

#[derive(Debug, Clone)]
pub enum PeerInput {
    PeerId(PeerId),
    Alias(String),
}

impl FromStr for PeerInput {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let value = value.trim();
        if value.is_empty() {
            return Err("Peer ID or alias cannot be empty".to_string());
        }

        if let Ok(peer_id) = value.parse::<PeerId>() {
            return Ok(Self::PeerId(peer_id));
        }

        Ok(Self::Alias(value.to_string()))
    }
}

impl fmt::Display for PeerInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PeerId(peer_id) => write!(f, "{peer_id}"),
            Self::Alias(alias) => write!(f, "{alias}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedPeerTarget {
    pub peer_id: String,
    pub alias: Option<String>,
    pub hostname: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct CliContext {
    #[serde(default)]
    current_peer_id: Option<String>,
    #[serde(default)]
    current_peer_alias: Option<String>,
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

    Err("No peer selected. Use --peer/-p or `fungi peer use <peer>`.".to_string())
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

pub fn get_current_peer(args: &CommonArgs) -> Result<Option<ResolvedPeerTarget>, String> {
    let Some(peer_id) = load_cli_context(args)?.current_peer_id else {
        return Ok(None);
    };
    resolve_peer_value(args, &peer_id).map(Some)
}

pub fn set_current_peer(args: &CommonArgs, peer: &ResolvedPeerTarget) -> Result<(), String> {
    let context = CliContext {
        current_peer_id: Some(peer.peer_id.clone()),
        current_peer_alias: peer.alias.clone(),
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
            let peer_info = lookup_peer_info(args, &peer_id.to_string())?;
            let alias = peer_info.as_ref().and_then(|entry| entry.alias.clone());
            if alias.is_none() {
                return Err(format!(
                    "Peer {} is not named yet. Name it first with `fungi device add {} --alias <name>` or `fungi device rename {} <name>`.",
                    peer_id, peer_id, peer_id
                ));
            }

            Ok(ResolvedPeerTarget {
                peer_id: peer_id.to_string(),
                alias,
                hostname: peer_info.and_then(|entry| entry.hostname.clone()),
            })
        }
        PeerInput::Alias(alias) => {
            let address_book = AddressBookConfig::apply_from_dir(&args.fungi_dir())
                .map_err(|error| format!("Failed to load address book: {error}"))?;
            match address_book.get_peer_by_alias(alias) {
                Some(entry) => Ok(ResolvedPeerTarget {
                    peer_id: entry.peer_id.to_string(),
                    alias: entry.alias.clone(),
                    hostname: entry.hostname.clone(),
                }),
                None => Err(format!(
                    "Unknown peer or alias: {alias}. Use --peer/-p with a peer ID, or set an address book alias first."
                )),
            }
        }
    }
}

pub fn resolve_peer_value(args: &CommonArgs, peer: &str) -> Result<ResolvedPeerTarget, String> {
    let peer = peer.parse::<PeerInput>()?;
    resolve_peer_input(args, &peer)
}

fn lookup_peer_info(
    args: &CommonArgs,
    peer_id: &str,
) -> Result<Option<fungi_config::address_book::PeerInfo>, String> {
    let address_book = AddressBookConfig::apply_from_dir(&args.fungi_dir())
        .map_err(|error| format!("Failed to load address book: {error}"))?;
    Ok(address_book
        .get_all_peers()
        .iter()
        .find(|entry| entry.peer_id.to_string() == peer_id)
        .cloned())
}

pub fn print_target_peer(peer: &ResolvedPeerTarget) {
    let alias = peer.alias.as_deref().unwrap_or("<unnamed>");
    match peer.hostname.as_deref() {
        Some(hostname) if !hostname.is_empty() => {
            eprintln!("Target peer: {alias} ({}) [{hostname}]", peer.peer_id)
        }
        _ => eprintln!("Target peer: {alias} ({})", peer.peer_id),
    }
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
