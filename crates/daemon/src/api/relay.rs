use anyhow::Result;
use fungi_config::EffectiveRelayAddress;
use libp2p::Multiaddr;

use crate::FungiDaemon;

impl FungiDaemon {
    pub fn relay_enabled(&self) -> bool {
        self.config().lock().network.relay_enabled
    }

    pub fn use_community_relays(&self) -> bool {
        self.config().lock().network.use_community_relays
    }

    pub fn custom_relay_addresses(&self) -> Vec<Multiaddr> {
        self.config().lock().network.custom_relay_addresses.clone()
    }

    pub fn effective_relay_addresses(&self) -> Vec<EffectiveRelayAddress> {
        self.config()
            .lock()
            .network
            .effective_relay_addresses(&fungi_swarm::get_default_relay_addrs())
    }

    pub fn set_relay_enabled(&self, enabled: bool) -> Result<()> {
        let current_config = self.config().lock().clone();
        let updated_config = current_config.set_relay_enabled(enabled)?;
        *self.config().lock() = updated_config;
        Ok(())
    }

    pub fn set_use_community_relays(&self, enabled: bool) -> Result<()> {
        let current_config = self.config().lock().clone();
        let updated_config = current_config.set_use_community_relays(enabled)?;
        *self.config().lock() = updated_config;
        Ok(())
    }

    pub fn add_custom_relay_address(&self, address: Multiaddr) -> Result<()> {
        let current_config = self.config().lock().clone();
        let updated_config = current_config.add_custom_relay_address(address)?;
        *self.config().lock() = updated_config;
        Ok(())
    }

    pub fn remove_custom_relay_address(&self, address: Multiaddr) -> Result<()> {
        let current_config = self.config().lock().clone();
        let updated_config = current_config.remove_custom_relay_address(&address)?;
        *self.config().lock() = updated_config;
        Ok(())
    }
}
