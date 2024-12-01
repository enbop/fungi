use libp2p_swarm::StreamProtocol;

pub const FUNGI_REMOTE_ACCESS_PROTOCOL: StreamProtocol =
    StreamProtocol::new("/fungi/remote-access/0.1.0");
pub const FUNGI_RELAY_HANDSHAKE_PROTOCOL: StreamProtocol = StreamProtocol::new("/fungi/relay-handshake/0.1.0");