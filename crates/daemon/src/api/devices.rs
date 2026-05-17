use anyhow::Result;
use fungi_config::devices::DeviceInfo;
use fungi_swarm::PeerAddressSource;
use libp2p::Multiaddr;
use libp2p::PeerId;

use crate::FungiDaemon;

impl FungiDaemon {
    pub async fn mdns_get_local_devices(&self) -> Result<Vec<DeviceInfo>> {
        let local_devices = self
            .mdns_control()
            .get_all_devices()
            .values()
            .cloned()
            .collect();
        Ok(local_devices)
    }

    pub fn devices_get_all(&self) -> Vec<DeviceInfo> {
        self.devices().lock().get_all_devices().clone()
    }

    pub fn devices_add_or_update(&self, device_info: DeviceInfo) -> Result<()> {
        let current_devices_config = self.devices().lock().clone();
        let updated_devices_config =
            current_devices_config.add_or_update_device(device_info.clone())?;
        *self.devices().lock() = updated_devices_config;
        self.hydrate_device_info(&device_info);
        Ok(())
    }

    pub fn devices_get_peer(&self, peer_id: PeerId) -> Option<DeviceInfo> {
        self.devices().lock().get_device_info(&peer_id).cloned()
    }

    pub fn devices_remove(&self, peer_id: PeerId) -> Result<()> {
        let current_devices_config = self.devices().lock().clone();
        let updated_devices_config = current_devices_config.remove_device(&peer_id)?;
        *self.devices().lock() = updated_devices_config;

        self.untrust_device(peer_id)?;
        self.remove_device_local_service_state(peer_id)?;
        Ok(())
    }

    fn remove_device_local_service_state(&self, peer_id: PeerId) -> Result<()> {
        let _ = self.forget_device_service_accesses(peer_id);

        let fungi_dir = self.config_fungi_dir()?;
        let published =
            fungi_config::service_cache::ServiceCache::apply_published_services_from_dir(
                &fungi_dir,
            )?;
        let managed =
            fungi_config::service_cache::ServiceCache::apply_managed_services_from_dir(&fungi_dir)?;
        let peer_id = peer_id.to_string();
        let _ = published.remove_device_services(&peer_id)?;
        let _ = managed.remove_device_services(&peer_id)?;
        Ok(())
    }

    pub fn list_trusted_devices(&self) -> Vec<DeviceInfo> {
        let trusted_device_ids = self
            .swarm_control()
            .state()
            .get_incoming_allowed_peers_list();
        let devices_config_guard = self.devices();
        let devices_config = devices_config_guard.lock();

        trusted_device_ids
            .into_iter()
            .map(
                |peer_id| match devices_config.get_device_info(&peer_id).cloned() {
                    Some(device_info) => device_info,
                    None => DeviceInfo::new_unknown(peer_id),
                },
            )
            .collect()
    }

    fn hydrate_device_info(&self, device_info: &DeviceInfo) {
        for address in &device_info.multiaddrs {
            match address.parse::<Multiaddr>() {
                Ok(multiaddr) => {
                    self.swarm_control().state().record_peer_address(
                        device_info.peer_id,
                        multiaddr,
                        PeerAddressSource::DeviceConfig,
                    );
                }
                Err(error) => {
                    log::debug!(
                        "Ignoring invalid device multiaddr for peer {}: {} ({})",
                        device_info.peer_id,
                        address,
                        error
                    );
                }
            }
        }
    }
}
