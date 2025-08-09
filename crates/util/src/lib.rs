pub mod ipc;
pub mod keypair;
pub mod protocols;

pub use sysinfo;

pub fn get_local_ip() -> Option<String> {
    if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0") {
        if let Ok(()) = socket.connect("8.8.8.8:80") {
            if let Ok(addr) = socket.local_addr() {
                return Some(addr.ip().to_string());
            }
        }
    }
    None
}
