pub mod ipc;
pub mod keypair;
pub mod protocols;

#[cfg(target_os = "android")]
mod mobile_device_info;
#[cfg(target_os = "android")]
pub use mobile_device_info::init_mobile_device_name;

pub fn get_local_ip() -> Option<String> {
    if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0")
        && let Ok(()) = socket.connect("8.8.8.8:80")
        && let Ok(addr) = socket.local_addr()
    {
        return Some(addr.ip().to_string());
    }
    None
}

pub fn get_hostname() -> Option<String> {
    #[cfg(target_os = "android")]
    {
        return mobile_device_info::get_mobile_device_name();
    }
    #[cfg(not(target_os = "android"))]
    sysinfo::System::host_name()
}
