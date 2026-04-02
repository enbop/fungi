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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forward_stdio_message_stdin_equality() {
        let a = ForwardStdioMessage::Stdin(b"hello".to_vec());
        let b = ForwardStdioMessage::Stdin(b"hello".to_vec());
        assert_eq!(a, b);
    }

    #[test]
    fn forward_stdio_message_variants_are_not_equal() {
        let stdin = ForwardStdioMessage::Stdin(b"data".to_vec());
        let stdout = ForwardStdioMessage::Stdout(b"data".to_vec());
        assert_ne!(stdin, stdout);
    }

    #[test]
    fn forward_stdio_message_empty_payload() {
        let msg = ForwardStdioMessage::Stderr(vec![]);
        assert_eq!(msg, ForwardStdioMessage::Stderr(vec![]));
    }

    #[test]
    fn forward_stdio_message_debug_contains_variant_name() {
        let msg = ForwardStdioMessage::Stdout(b"out".to_vec());
        let dbg = format!("{msg:?}");
        assert!(dbg.contains("Stdout"));
    }

    #[test]
    fn daemon_message_remote_response_ok_equality() {
        let a = DaemonMessage::RemoteResponse(Ok("result".to_string()));
        let b = DaemonMessage::RemoteResponse(Ok("result".to_string()));
        assert_eq!(a, b);
    }

    #[test]
    fn daemon_message_remote_response_err_equality() {
        let a = DaemonMessage::RemoteResponse(Err("failure".to_string()));
        let b = DaemonMessage::RemoteResponse(Err("failure".to_string()));
        assert_eq!(a, b);
    }

    #[test]
    fn daemon_message_ok_and_err_are_not_equal() {
        let ok = DaemonMessage::RemoteResponse(Ok("x".to_string()));
        let err = DaemonMessage::RemoteResponse(Err("x".to_string()));
        assert_ne!(ok, err);
    }

    #[test]
    fn daemon_message_remote_request_contains_peer_id() {
        let peer_id = PeerId::random();
        let msg = DaemonMessage::RemoteRequest(peer_id);
        let dbg = format!("{msg:?}");
        assert!(dbg.contains("RemoteRequest"));
    }
}
