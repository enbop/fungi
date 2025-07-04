use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use anyhow::{Result, bail};
use fungi_config::tcp_tunneling::{ForwardingRule, ListeningRule, TcpTunneling};
use fungi_util::protocols::FUNGI_TUNNEL_PROTOCOL;
use libp2p::{PeerId, StreamProtocol};
use libp2p_stream::Control;
use parking_lot::Mutex;
use tokio::task::JoinHandle;

/// State for active forwarding rules
#[derive(Debug)]
struct ForwardingRuleState {
    rule: ForwardingRule,
    task_handle: JoinHandle<()>,
}

/// State for active listening rules  
#[derive(Debug)]
struct ListeningRuleState {
    rule: ListeningRule,
    task_handle: JoinHandle<()>,
}

/// Control interface for TCP tunneling functionality
/// Manages both forwarding (local port -> remote peer) and listening (remote peer -> local port) rules
#[derive(Clone)]
pub struct TcpTunnelingControl {
    stream_control: Control,
    forwarding_rules: Arc<Mutex<HashMap<String, ForwardingRuleState>>>,
    listening_rules: Arc<Mutex<HashMap<String, ListeningRuleState>>>,
}

impl TcpTunnelingControl {
    pub fn new(stream_control: Control) -> Self {
        Self {
            stream_control,
            forwarding_rules: Arc::new(Mutex::new(HashMap::new())),
            listening_rules: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Initialize TCP tunneling from config
    pub fn init_from_config(&self, config: &TcpTunneling) {
        if config.forwarding.enabled {
            for rule in &config.forwarding.rules {
                if let Err(e) = self.add_forwarding_rule(rule.clone()) {
                    log::warn!("Failed to add forwarding rule: {}", e);
                }
            }
        }

        if config.listening.enabled {
            for rule in &config.listening.rules {
                if let Err(e) = self.add_listening_rule(rule.clone()) {
                    log::warn!("Failed to add listening rule: {}", e);
                }
            }
        }
    }

    /// Add a new forwarding rule (local port -> remote peer)
    pub fn add_forwarding_rule(&self, rule: ForwardingRule) -> Result<String> {
        let rule_id = self.generate_forwarding_rule_id(&rule);

        let mut rules = self.forwarding_rules.lock();
        if rules.contains_key(&rule_id) {
            bail!("Forwarding rule already exists: {}", rule_id);
        }

        let local_addr: SocketAddr = (&rule.local_socket)
            .try_into()
            .map_err(|e| anyhow::anyhow!("Invalid local socket address: {}", e))?;

        let target_peer: PeerId = rule
            .remote
            .peer_id
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid peer ID: {}", e))?;

        let target_protocol = StreamProtocol::try_from_owned(format!(
            "{}/{}",
            FUNGI_TUNNEL_PROTOCOL, rule.remote.port
        ))
        .map_err(|e| anyhow::anyhow!("Invalid protocol: {}", e))?;

        let stream_control = self.stream_control.clone();

        log::info!(
            "Adding forwarding rule: {} -> {}/{}",
            local_addr,
            target_peer,
            target_protocol
        );

        let task_handle = tokio::spawn(async move {
            fungi_util::tcp_tunneling::forward_port_to_peer(
                stream_control,
                local_addr,
                target_peer,
                target_protocol,
            )
            .await;
        });

        let rule_state = ForwardingRuleState { rule, task_handle };

        rules.insert(rule_id.clone(), rule_state);
        Ok(rule_id)
    }

    /// Remove a forwarding rule by ID
    pub fn remove_forwarding_rule(&self, rule_id: &str) -> Result<()> {
        let mut rules = self.forwarding_rules.lock();
        if let Some(rule_state) = rules.remove(rule_id) {
            rule_state.task_handle.abort();
            log::info!("Removed forwarding rule: {}", rule_id);
            Ok(())
        } else {
            bail!("Forwarding rule not found: {}", rule_id);
        }
    }

    /// Add a new listening rule (remote peer -> local port)
    pub fn add_listening_rule(&self, rule: ListeningRule) -> Result<String> {
        let rule_id = self.generate_listening_rule_id(&rule);

        let mut rules = self.listening_rules.lock();
        if rules.contains_key(&rule_id) {
            bail!("Listening rule already exists: {}", rule_id);
        }

        let local_addr: SocketAddr = (&rule.local_socket)
            .try_into()
            .map_err(|e| anyhow::anyhow!("Invalid local socket address: {}", e))?;

        let listening_protocol = StreamProtocol::try_from_owned(format!(
            "{}/{}",
            FUNGI_TUNNEL_PROTOCOL, rule.listening_port
        ))
        .map_err(|e| anyhow::anyhow!("Invalid protocol: {}", e))?;

        let stream_control = self.stream_control.clone();

        log::info!(
            "Adding listening rule: {} for {}",
            local_addr,
            listening_protocol
        );

        let task_handle = tokio::spawn(async move {
            fungi_util::tcp_tunneling::listen_p2p_to_port(
                stream_control,
                listening_protocol,
                local_addr,
            )
            .await;
        });

        let rule_state = ListeningRuleState { rule, task_handle };

        rules.insert(rule_id.clone(), rule_state);
        Ok(rule_id)
    }

    /// Remove a listening rule by ID
    pub fn remove_listening_rule(&self, rule_id: &str) -> Result<()> {
        let mut rules = self.listening_rules.lock();
        if let Some(rule_state) = rules.remove(rule_id) {
            rule_state.task_handle.abort();
            log::info!("Removed listening rule: {}", rule_id);
            Ok(())
        } else {
            bail!("Listening rule not found: {}", rule_id);
        }
    }

    /// Get all active forwarding rules
    pub fn get_forwarding_rules(&self) -> Vec<(String, ForwardingRule)> {
        self.forwarding_rules
            .lock()
            .iter()
            .map(|(id, state)| (id.clone(), state.rule.clone()))
            .collect()
    }

    /// Get all active listening rules
    pub fn get_listening_rules(&self) -> Vec<(String, ListeningRule)> {
        self.listening_rules
            .lock()
            .iter()
            .map(|(id, state)| (id.clone(), state.rule.clone()))
            .collect()
    }

    /// Stop all active rules
    pub fn stop_all(&self) {
        let mut forwarding_rules = self.forwarding_rules.lock();
        for (rule_id, rule_state) in forwarding_rules.drain() {
            log::info!("Stopping forwarding rule: {}", rule_id);
            rule_state.task_handle.abort();
        }

        let mut listening_rules = self.listening_rules.lock();
        for (rule_id, rule_state) in listening_rules.drain() {
            log::info!("Stopping listening rule: {}", rule_id);
            rule_state.task_handle.abort();
        }
    }

    /// Generate unique ID for forwarding rule
    fn generate_forwarding_rule_id(&self, rule: &ForwardingRule) -> String {
        format!(
            "forward_{}:{}_to_{}",
            rule.local_socket.host, rule.local_socket.port, rule.remote.peer_id
        )
    }

    /// Generate unique ID for listening rule
    fn generate_listening_rule_id(&self, rule: &ListeningRule) -> String {
        format!(
            "listen_{}:{}_for_port_{}",
            rule.local_socket.host, rule.local_socket.port, rule.listening_port
        )
    }
}
