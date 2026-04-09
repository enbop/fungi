use libp2p_identity::PeerId;
use multiaddr::Multiaddr;
use serde::{Deserialize, Serialize};

#[cfg(target_os = "android")]
const fn default_idle_connection_timeout_secs() -> u64 {
    90
}

#[cfg(not(target_os = "android"))]
const fn default_idle_connection_timeout_secs() -> u64 {
    300
}

const fn default_relay_enabled() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelayAddressSource {
    Community,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveRelayAddress {
    pub address: Multiaddr,
    pub source: RelayAddressSource,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Network {
    #[serde(default)]
    pub listen_tcp_port: u16,
    #[serde(default)]
    pub listen_udp_port: u16,
    #[serde(default)]
    pub incoming_allowed_peers: Vec<PeerId>,
    #[serde(default = "default_relay_enabled")]
    pub relay_enabled: bool,
    #[serde(default = "default_use_community_relays")]
    pub use_community_relays: bool,
    #[serde(default)]
    pub custom_relay_addresses: Vec<Multiaddr>,
    #[serde(default = "default_idle_connection_timeout_secs")]
    pub idle_connection_timeout_secs: u64,
}

const fn default_use_community_relays() -> bool {
    true
}

impl Network {
    pub fn effective_relay_addresses(
        &self,
        community_relays: &[Multiaddr],
    ) -> Vec<EffectiveRelayAddress> {
        if !self.relay_enabled {
            return Vec::new();
        }

        let mut effective = Vec::new();

        if self.use_community_relays {
            for address in community_relays {
                if effective
                    .iter()
                    .any(|entry: &EffectiveRelayAddress| entry.address == *address)
                {
                    continue;
                }

                effective.push(EffectiveRelayAddress {
                    address: address.clone(),
                    source: RelayAddressSource::Community,
                });
            }
        }

        for address in &self.custom_relay_addresses {
            if effective
                .iter()
                .any(|entry: &EffectiveRelayAddress| entry.address == *address)
            {
                continue;
            }

            effective.push(EffectiveRelayAddress {
                address: address.clone(),
                source: RelayAddressSource::Custom,
            });
        }

        effective
    }
}

impl Default for Network {
    fn default() -> Self {
        Self {
            listen_tcp_port: 0,
            listen_udp_port: 0,
            incoming_allowed_peers: Vec::new(),
            relay_enabled: default_relay_enabled(),
            use_community_relays: default_use_community_relays(),
            custom_relay_addresses: Vec::new(),
            idle_connection_timeout_secs: default_idle_connection_timeout_secs(),
        }
    }
}
