use std::{
    collections::HashMap,
    sync::Arc,
    sync::mpsc,
    time::{Duration, SystemTime},
};

use anyhow::Result;
use libp2p::PeerId;
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use parking_lot::Mutex;

const FUNGI_SERVICE_TYPE: &str = "_fungi._tcp.local.";
const DEVICE_TIMEOUT_SECONDS: u64 = 300; // 5 minutes

#[derive(Clone)]
pub enum Os {
    Windows,
    MacOS,
    Linux,
    Android,
    IOS,
}

impl Os {
    pub fn this_device() -> Self {
        if cfg!(target_os = "windows") {
            Os::Windows
        } else if cfg!(target_os = "macos") {
            Os::MacOS
        } else if cfg!(target_os = "linux") {
            Os::Linux
        } else if cfg!(target_os = "android") {
            Os::Android
        } else if cfg!(target_os = "ios") {
            Os::IOS
        } else {
            Os::Linux
        }
    }
}

impl Into<String> for Os {
    fn into(self) -> String {
        match self {
            Os::Windows => "Windows".to_string(),
            Os::MacOS => "MacOS".to_string(),
            Os::Linux => "Linux".to_string(),
            Os::Android => "Android".to_string(),
            Os::IOS => "iOS".to_string(),
        }
    }
}

impl TryFrom<&str> for Os {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "Windows" => Ok(Os::Windows),
            "MacOS" => Ok(Os::MacOS),
            "Linux" => Ok(Os::Linux),
            "Android" => Ok(Os::Android),
            "iOS" => Ok(Os::IOS),
            _ => Err(format!("Unknown OS: {}", value)),
        }
    }
}

#[derive(Clone)]
pub struct DeviceInfo {
    peer_id: PeerId,
    hostname: Option<String>,
    os: Os,
    version: String,
    ip_address: Option<String>,
    created_at: SystemTime,
}

impl DeviceInfo {
    pub fn new(peer_id: PeerId) -> Self {
        let version = std::env!("CARGO_PKG_VERSION").to_string();

        DeviceInfo {
            peer_id,
            hostname: fungi_util::sysinfo::System::host_name(),
            os: Os::this_device(),
            version,
            ip_address: MdnsControl::get_local_ip(),
            created_at: SystemTime::now(),
        }
    }

    pub fn peer_id(&self) -> &PeerId {
        &self.peer_id
    }

    pub fn hostname(&self) -> Option<&String> {
        self.hostname.as_ref()
    }

    pub fn os(&self) -> &Os {
        &self.os
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    pub fn ip_address(&self) -> Option<&String> {
        self.ip_address.as_ref()
    }

    pub fn created_at(&self) -> SystemTime {
        self.created_at
    }

    pub fn is_expired(&self) -> bool {
        if let Ok(elapsed) = self.created_at.elapsed() {
            elapsed.as_secs() > DEVICE_TIMEOUT_SECONDS
        } else {
            true // If we can't determine elapsed time, consider it expired
        }
    }
}

#[derive(Clone)]
pub struct MdnsControl {
    local_devices: Arc<Mutex<HashMap<PeerId, DeviceInfo>>>,
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

    pub fn start(&self, peer_id: PeerId) -> Result<()> {
        self.stop();

        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        *self.shutdown_tx.lock() = Some(shutdown_tx);

        let local_devices = Arc::clone(&self.local_devices);
        let task_handle = Arc::clone(&self.task);

        let device_info = DeviceInfo::new(peer_id);
        local_devices.lock().insert(peer_id, device_info.clone());

        let handle = std::thread::spawn(move || {
            if let Err(e) = Self::run_mdns_service(peer_id, device_info, local_devices, shutdown_rx)
            {
                eprintln!("mDNS service error: {}", e);
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

    pub fn get_all_devices(&self) -> HashMap<PeerId, DeviceInfo> {
        self.cleanup_expired_devices();
        self.local_devices.lock().clone()
    }

    pub fn get_device(&self, peer_id: &PeerId) -> Option<DeviceInfo> {
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
        peer_id: PeerId,
        device_info: DeviceInfo,
        local_devices: Arc<Mutex<HashMap<PeerId, DeviceInfo>>>,
        shutdown_rx: mpsc::Receiver<()>,
    ) -> Result<()> {
        let mdns = ServiceDaemon::new()?;
        let service_type = FUNGI_SERVICE_TYPE;

        let local_ip = Self::get_local_ip().unwrap_or_else(|| "127.0.0.1".to_string());
        let host_name = format!("{}.local.", local_ip);
        let instance_name = peer_id.to_string();
        let port = 0;

        let os_string: String = device_info.os().clone().into();
        let peer_id_string = peer_id.to_string();
        let hostname_str = device_info
            .hostname()
            .map(|s| s.as_str())
            .unwrap_or("unknown");
        let version_str = device_info.version();

        let properties = [
            ("peer_id", peer_id_string.as_str()),
            ("hostname", hostname_str),
            ("os", os_string.as_str()),
            ("version", version_str),
        ];

        let service_info = ServiceInfo::new(
            service_type,
            &instance_name,
            &host_name,
            &local_ip,
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
                            if remote_device.peer_id != peer_id {
                                log::debug!("Discovered device: {:?}", remote_device.peer_id);
                                local_devices
                                    .lock()
                                    .insert(remote_device.peer_id, remote_device);
                            }
                        }
                    }
                    ServiceEvent::ServiceRemoved(_typ, _fullname) => {
                        // TODO using this patch may cause too many ServiceRemoved events
                        // Cargo.toml
                        // [patch.crates-io]
                        // # workaround for fixing the build error on macOS
                        // if-watch = { git = "https://github.com/Heap-Hop/if-watch.git", branch = "no_system_configuration_on_macos" }

                        // since we have the expired check, workaround to ignore this event

                        // https://github.com/keepsimple1/mdns-sd/issues/363
                        // log::info!("Service removed: {} of type {}", fullname, typ);
                        // Self::remove_device_by_fullname(&local_devices, &fullname);
                    }
                    other_event => {
                        log::debug!("Received other mDNS event: {:?}", other_event);
                    }
                },
                Err(_) => {
                    // Timeout or other error, continue to check shutdown signal
                    continue;
                }
            }
        }

        mdns.shutdown()?;
        Ok(())
    }

    fn get_local_ip() -> Option<String> {
        if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0") {
            if let Ok(()) = socket.connect("8.8.8.8:80") {
                if let Ok(addr) = socket.local_addr() {
                    return Some(addr.ip().to_string());
                }
            }
        }

        None
    }

    fn parse_service_info(info: &mdns_sd::ServiceInfo) -> Option<DeviceInfo> {
        let properties = info.get_properties();

        let peer_id_str = properties.get("peer_id")?.val_str();
        let peer_id = peer_id_str.parse::<PeerId>().ok()?;

        let hostname = properties.get("hostname").map(|s| s.val_str().to_string());
        let os_str = properties.get("os")?.val_str();
        let os = Os::try_from(os_str).ok()?;
        let version = properties.get("version")?.val_str().to_string();

        // Get IP address from service info
        let ip_address = Some(info.get_addresses().iter().next()?.to_string());

        Some(DeviceInfo {
            peer_id,
            hostname,
            os,
            version,
            ip_address,
            created_at: SystemTime::now(),
        })
    }

    // TODO: check if this is working correctly
    fn _remove_device_by_fullname(
        local_devices: &Arc<Mutex<HashMap<PeerId, DeviceInfo>>>,
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
