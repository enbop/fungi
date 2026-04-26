use anyhow::Result;
use fungi_config::address_book::PeerInfo;
use fungi_swarm::PeerAddressSource;
use libp2p::Multiaddr;
use libp2p::PeerId;

use crate::FungiDaemon;

impl FungiDaemon {
    pub async fn mdns_get_local_devices(&self) -> Result<Vec<PeerInfo>> {
        let local_devices = self
            .mdns_control()
            .get_all_devices()
            .values()
            .cloned()
            .collect();
        Ok(local_devices)
    }

    pub fn address_book_get_all(&self) -> Vec<PeerInfo> {
        self.address_book().lock().get_all_peers().clone()
    }

    pub fn address_book_add_or_update(&self, peer_info: PeerInfo) -> Result<()> {
        let current_peers_config = self.address_book().lock().clone();
        let updated_peers_config = current_peers_config.add_or_update_peer(peer_info.clone())?;
        *self.address_book().lock() = updated_peers_config;
        self.hydrate_address_book_peer_info(&peer_info);
        Ok(())
    }

    pub fn address_book_get_peer(&self, peer_id: PeerId) -> Option<PeerInfo> {
        self.address_book().lock().get_peer_info(&peer_id).cloned()
    }

    pub fn address_book_remove(&self, peer_id: PeerId) -> Result<()> {
        let current_peers_config = self.address_book().lock().clone();
        let updated_peers_config = current_peers_config.remove_peer(&peer_id)?;
        *self.address_book().lock() = updated_peers_config;
        Ok(())
    }

    pub fn get_incoming_allowed_peers(&self) -> Vec<PeerInfo> {
        let allowed_peers = self
            .swarm_control()
            .state()
            .get_incoming_allowed_peers_list();
        let peers_config_guard = self.address_book();
        let peers_config = peers_config_guard.lock();

        allowed_peers
            .into_iter()
            .map(
                |peer_id| match peers_config.get_peer_info(&peer_id).cloned() {
                    Some(peer_info) => peer_info,
                    None => PeerInfo::new_unknown(peer_id),
                },
            )
            .collect()
    }

    fn hydrate_address_book_peer_info(&self, peer_info: &PeerInfo) {
        for address in &peer_info.multiaddrs {
            match address.parse::<Multiaddr>() {
                Ok(multiaddr) => {
                    self.swarm_control().state().record_peer_address(
                        peer_info.peer_id,
                        multiaddr,
                        PeerAddressSource::AddressBook,
                    );
                }
                Err(error) => {
                    log::debug!(
                        "Ignoring invalid address book multiaddr for peer {}: {} ({})",
                        peer_info.peer_id,
                        address,
                        error
                    );
                }
            }
        }
    }
}
