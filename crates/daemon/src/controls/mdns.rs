use std::{
    collections::HashMap,
    sync::{Arc, mpsc},
    time::Duration,
    vec,
};

use anyhow::Result;
use fungi_config::address_book::PeerInfo;
use libp2p::PeerId;
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use parking_lot::Mutex;

const FUNGI_SERVICE_TYPE: &str = "_fungi._tcp.local.";

#[derive(Clone)]
pub struct MdnsControl {
    local_devices: Arc<Mutex<HashMap<PeerId, PeerInfo>>>,
    task: Arc<Mutex<Option<std::thread::JoinHandle<()>>>>,
    shutdown_tx: Arc<Mutex<Option<mpsc::Sender<()>>>>,
}

impl MdnsControl {
    pub fn new() -> Self {
        Self {
            local_devices: Arc::new(Mutex::new(HashMap::new())),
            task: Arc::new(Mutex::new(None)),
            shutdown_tx: Arc::new(Mutex::new(None)),
        }
    }

    pub fn start(&self, peer_info: PeerInfo) -> Result<()> {
        self.stop();

        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        *self.shutdown_tx.lock() = Some(shutdown_tx);

        let local_devices = Arc::clone(&self.local_devices);
        let task_handle = Arc::clone(&self.task);

        let handle = std::thread::spawn(move || {
            if let Err(e) = Self::run_mdns_service(peer_info, local_devices, shutdown_rx) {
                log::error!("mDNS service error: {}", e);
            }
        });

        *task_handle.lock() = Some(handle);
        Ok(())
    }

    pub fn stop(&self) {
        // Send shutdown signal
        if let Some(tx) = self.shutdown_tx.lock().take() {
            let _ = tx.send(());
        }

        // Wait for thread to finish
        if let Some(handle) = self.task.lock().take() {
            let _ = handle.join();
        }

        self.local_devices.lock().clear();
    }

    pub fn get_all_devices(&self) -> HashMap<PeerId, PeerInfo> {
        self.cleanup_expired_devices();
        self.local_devices.lock().clone()
    }

    pub fn get_device(&self, peer_id: &PeerId) -> Option<PeerInfo> {
        self.cleanup_expired_devices();
        self.local_devices.lock().get(peer_id).cloned()
    }

    pub fn get_device_count(&self) -> usize {
        self.cleanup_expired_devices();
        self.local_devices.lock().len()
    }

    fn cleanup_expired_devices(&self) {
        let mut devices = self.local_devices.lock();
        devices.retain(|_, device| !device.is_expired());
    }

    fn run_mdns_service(
        device_info: PeerInfo,
        local_devices: Arc<Mutex<HashMap<PeerId, PeerInfo>>>,
        shutdown_rx: mpsc::Receiver<()>,
    ) -> Result<()> {
        let mdns = ServiceDaemon::new()?;
        let service_type = FUNGI_SERVICE_TYPE;

        let current_peer_id = device_info.peer_id;

        let instance_name = current_peer_id.to_string();
        let port = 0;

        let mut properties = Vec::new();
        properties.push(("peer_id", current_peer_id.to_string()));
        properties.push(("os", (&device_info.os).into()));
        properties.push(("version", device_info.version.to_string()));

        if let Some(host_name) = device_info.hostname.as_ref() {
            properties.push(("hostname", host_name.to_string()));
        }

        let service_info = ServiceInfo::new(
            service_type,
            &instance_name,
            &format!("{}.local.", instance_name),
            &device_info.private_ips.get(0).cloned().unwrap_or_default(),
            port,
            &properties[..],
        )?;

        mdns.register(service_info)?;
        let receiver = mdns.browse(service_type)?;

        loop {
            // Check for shutdown signal with timeout
            match shutdown_rx.try_recv() {
                Ok(_) => {
                    log::info!("Received shutdown signal, stopping mDNS service");
                    break;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    log::info!("Shutdown channel disconnected, stopping mDNS service");
                    break;
                }
                Err(mpsc::TryRecvError::Empty) => {
                    // No shutdown signal, continue
                }
            }

            // Use recv_timeout to avoid blocking indefinitely
            match receiver.recv_timeout(Duration::from_millis(100)) {
                Ok(event) => match event {
                    ServiceEvent::ServiceResolved(info) => {
                        if let Some(remote_device) = Self::parse_service_info(&info) {
                            if remote_device.peer_id != current_peer_id {
                                log::info!("Discovered device: {:?}", remote_device.peer_id);
                                local_devices
                                    .lock()
                                    .insert(remote_device.peer_id.to_owned(), remote_device);
                            }
                        }
                    }
                    ServiceEvent::ServiceRemoved(typ, fullname) => {
                        log::info!("Service removed: {} of type {}", fullname, typ);
                        Self::remove_device_by_fullname(&local_devices, &fullname);
                    }
                    other_event => {
                        log::debug!("Received other mDNS event: {:?}", other_event);
                    }
                },
                Err(flume::RecvTimeoutError::Timeout) => {
                    continue;
                }
                Err(flume::RecvTimeoutError::Disconnected) => {
                    log::info!("mDNS receiver disconnected, stopping service");
                    break;
                }
            }
        }

        mdns.shutdown()?;
        Ok(())
    }

    fn parse_service_info(info: &mdns_sd::ServiceInfo) -> Option<PeerInfo> {
        let properties = info.get_properties();

        let mut peer_info: PeerInfo = properties.try_into().ok()?;

        // Get IP address from service info
        let private_ips = Some(info.get_addresses().iter().next()?.to_string())
            .map(|addr| vec![addr])
            .unwrap_or_default();

        peer_info.private_ips = private_ips;

        Some(peer_info)
    }

    fn remove_device_by_fullname(
        local_devices: &Arc<Mutex<HashMap<PeerId, PeerInfo>>>,
        fullname: &str,
    ) {
        if let Some(instance_name) = fullname.split('.').next() {
            if let Ok(peer_id) = instance_name.parse::<PeerId>() {
                local_devices.lock().remove(&peer_id);
            }
        }
    }
}

impl Default for MdnsControl {
    fn default() -> Self {
        Self::new()
    }
}
