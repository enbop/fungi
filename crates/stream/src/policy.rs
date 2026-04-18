use std::{collections::HashSet, sync::Arc};

use libp2p_identity::PeerId;
use parking_lot::RwLock;

pub type SharedPeerAllowList = Arc<RwLock<HashSet<PeerId>>>;

#[derive(Debug, Clone)]
pub(crate) enum GlobalAllowPolicy {
    AllowAll,
    PeerSet(SharedPeerAllowList),
}

impl GlobalAllowPolicy {
    pub(crate) fn allow_all() -> Self {
        Self::AllowAll
    }

    pub(crate) fn peer_set(peers: SharedPeerAllowList) -> Self {
        Self::PeerSet(peers)
    }
}

#[derive(Debug, Clone, Default)]
pub enum ProtocolAllowList {
    // `*` in config / API terms: reuse the outer global allow list as-is.
    #[default]
    InheritGlobal,
    PeerSet(SharedPeerAllowList),
}

impl ProtocolAllowList {
    pub fn inherit_global() -> Self {
        Self::InheritGlobal
    }

    pub fn peer_set(peers: SharedPeerAllowList) -> Self {
        Self::PeerSet(peers)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorizationRejectReason {
    RejectedByGlobalAllowList,
    RejectedByProtocolAllowList,
}

pub(crate) fn authorize_inbound(
    global_allow_policy: &GlobalAllowPolicy,
    protocol_allow_list: &ProtocolAllowList,
    peer_id: PeerId,
) -> Result<(), AuthorizationRejectReason> {
    // Authorization is intentionally ordered as:
    // 1. global allow list
    // 2. protocol-local allow list
    // so a protocol can only be stricter than the node-wide trust boundary.
    match global_allow_policy {
        GlobalAllowPolicy::AllowAll => {}
        GlobalAllowPolicy::PeerSet(peers) => {
            if !peers.read().contains(&peer_id) {
                return Err(AuthorizationRejectReason::RejectedByGlobalAllowList);
            }
        }
    }

    match protocol_allow_list {
        ProtocolAllowList::InheritGlobal => Ok(()),
        ProtocolAllowList::PeerSet(peers) => peers
            .read()
            .contains(&peer_id)
            .then_some(())
            .ok_or(AuthorizationRejectReason::RejectedByProtocolAllowList),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shared_allow_list(peers: impl IntoIterator<Item = PeerId>) -> SharedPeerAllowList {
        Arc::new(RwLock::new(peers.into_iter().collect()))
    }

    #[test]
    fn inherit_global_allows_when_peer_is_globally_allowed() {
        let peer = PeerId::random();
        let global = GlobalAllowPolicy::peer_set(shared_allow_list([peer]));

        assert_eq!(
            authorize_inbound(&global, &ProtocolAllowList::inherit_global(), peer),
            Ok(())
        );
    }

    #[test]
    fn global_allow_list_rejects_before_protocol_allow_list() {
        let peer = PeerId::random();
        let global = GlobalAllowPolicy::peer_set(shared_allow_list([]));
        let protocol = ProtocolAllowList::peer_set(shared_allow_list([peer]));

        assert_eq!(
            authorize_inbound(&global, &protocol, peer),
            Err(AuthorizationRejectReason::RejectedByGlobalAllowList)
        );
    }

    #[test]
    fn protocol_allow_list_can_be_stricter_than_global() {
        let peer = PeerId::random();
        let global = GlobalAllowPolicy::peer_set(shared_allow_list([peer]));
        let protocol = ProtocolAllowList::peer_set(shared_allow_list([]));

        assert_eq!(
            authorize_inbound(&global, &protocol, peer),
            Err(AuthorizationRejectReason::RejectedByProtocolAllowList)
        );
    }

    #[test]
    fn allow_all_global_policy_skips_global_peer_check() {
        let peer = PeerId::random();

        assert_eq!(
            authorize_inbound(
                &GlobalAllowPolicy::allow_all(),
                &ProtocolAllowList::inherit_global(),
                peer,
            ),
            Ok(())
        );
    }
}
