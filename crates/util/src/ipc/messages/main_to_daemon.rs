use libp2p_identity::PeerId;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum ForwardStdioMessage {
    Stdin(Vec<u8>),
    Stdout(Vec<u8>),
    Stderr(Vec<u8>),
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum DaemonMessage {
    RemoteRequest(PeerId),
    RemoteResponse(Result<String, String>),
}
