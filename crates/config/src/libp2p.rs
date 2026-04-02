use libp2p_identity::PeerId;
use multiaddr::Multiaddr;
use serde::{Deserialize, Serialize};

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_network_has_zero_ports() {
        let n = Network::default();
        assert_eq!(n.listen_tcp_port, 0);
        assert_eq!(n.listen_udp_port, 0);
    }

    #[test]
    fn default_network_has_no_allowed_peers() {
        let n = Network::default();
        assert!(n.incoming_allowed_peers.is_empty());
    }

    #[test]
    fn default_network_relay_is_enabled() {
        let n = Network::default();
        assert!(n.relay_enabled);
        assert!(n.use_community_relays);
    }

    #[test]
    fn default_network_idle_timeout_is_300_secs() {
        let n = Network::default();
        assert_eq!(n.idle_connection_timeout_secs, 300);
    }

    #[test]
    fn default_network_no_custom_relay_addresses() {
        let n = Network::default();
        assert!(n.custom_relay_addresses.is_empty());
    }

    #[test]
    fn network_deserializes_from_empty_toml() {
        let toml = "[network]\n";
        let config: toml::Value = toml::from_str(toml).unwrap();
        let network: Network = config["network"].clone().try_into().unwrap();
        assert_eq!(network.idle_connection_timeout_secs, 300);
        assert!(network.relay_enabled);
        assert!(network.use_community_relays);
    }

    #[test]
    fn network_deserializes_custom_ports() {
        let toml = "[network]\nlisten_tcp_port = 7000\nlisten_udp_port = 7001\n";
        let config: toml::Value = toml::from_str(toml).unwrap();
        let network: Network = config["network"].clone().try_into().unwrap();
        assert_eq!(network.listen_tcp_port, 7000);
        assert_eq!(network.listen_udp_port, 7001);
    }

    #[test]
    fn network_relay_disabled_via_relay_enabled_false() {
        let toml = "[network]\nrelay_enabled = false\n";
        let config: toml::Value = toml::from_str(toml).unwrap();
        let network: Network = config["network"].clone().try_into().unwrap();
        assert!(!network.relay_enabled);
        assert!(
            network
                .effective_relay_addresses(&[])
                .is_empty()
        );
    }

    #[test]
    fn network_relay_disabled_returns_no_effective_addresses() {
        let mut n = Network::default();
        n.relay_enabled = false;
        let community: Vec<multiaddr::Multiaddr> =
            vec!["/ip4/1.2.3.4/tcp/443".parse().unwrap()];
        assert!(n.effective_relay_addresses(&community).is_empty());
    }

    #[test]
    fn network_relay_enabled_with_community_includes_community_relays() {
        let n = Network::default();
        let community: Vec<multiaddr::Multiaddr> =
            vec!["/ip4/1.2.3.4/tcp/443".parse().unwrap()];
        let effective = n.effective_relay_addresses(&community);
        assert_eq!(effective.len(), 1);
        assert_eq!(effective[0].source, RelayAddressSource::Community);
    }

    #[test]
    fn network_relay_custom_addresses_are_tagged_as_custom() {
        let mut n = Network::default();
        n.use_community_relays = false;
        let custom_addr: multiaddr::Multiaddr = "/ip4/5.6.7.8/tcp/9000".parse().unwrap();
        n.custom_relay_addresses.push(custom_addr.clone());
        let effective = n.effective_relay_addresses(&[]);
        assert_eq!(effective.len(), 1);
        assert_eq!(effective[0].source, RelayAddressSource::Custom);
        assert_eq!(effective[0].address, custom_addr);
    }

    #[test]
    fn network_deserializes_custom_idle_timeout() {
        let toml = "[network]\nidle_connection_timeout_secs = 60\n";
        let config: toml::Value = toml::from_str(toml).unwrap();
        let network: Network = config["network"].clone().try_into().unwrap();
        assert_eq!(network.idle_connection_timeout_secs, 60);
    }
}
