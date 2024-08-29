pub mod local_listener;
pub mod peer_listener;

use libp2p::StreamProtocol;

const FUNGI_REMOTE_ACCESS_PROTOCOL: StreamProtocol =
    StreamProtocol::new("/fungi/remote-access/0.1.0");
