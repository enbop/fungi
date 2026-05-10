use std::{collections::HashSet, sync::Arc, time::Duration};

use futures::{AsyncReadExt as _, AsyncWriteExt as _, StreamExt as _};
use libp2p_identity::PeerId;
use libp2p_swarm::{
    ConnectionId, Swarm, SwarmEvent,
    dial_opts::{DialOpts, PeerCondition},
};
use libp2p_swarm_test::SwarmExt as _;
use parking_lot::RwLock;

use crate::{Behaviour, ProtocolAllowList, SharedPeerAllowList, control::OpenStreamError};

const PROTOCOL: libp2p_swarm::StreamProtocol =
    libp2p_swarm::StreamProtocol::new("/fungi-stream/test/1.0.0");

fn shared_allow_list(peers: impl IntoIterator<Item = PeerId>) -> SharedPeerAllowList {
    Arc::new(RwLock::new(peers.into_iter().collect::<HashSet<_>>()))
}

async fn connect_and_get_connection_ids(
    swarm1: &mut Swarm<Behaviour>,
    swarm2: &mut Swarm<Behaviour>,
) -> (ConnectionId, ConnectionId) {
    let dial_opts = DialOpts::peer_id(*swarm2.local_peer_id())
        .addresses(swarm2.external_addresses().cloned().collect())
        .condition(PeerCondition::Always)
        .build();

    swarm1.dial(dial_opts).unwrap();

    let mut dialer_connection_id = None;
    let mut listener_connection_id = None;

    loop {
        match futures::future::select(swarm1.next_swarm_event(), swarm2.next_swarm_event()).await {
            futures::future::Either::Left((
                SwarmEvent::ConnectionEstablished { connection_id, .. },
                _,
            )) => {
                dialer_connection_id = Some(connection_id);
            }
            futures::future::Either::Right((
                SwarmEvent::ConnectionEstablished { connection_id, .. },
                _,
            )) => {
                listener_connection_id = Some(connection_id);
            }
            futures::future::Either::Left((_event, _)) => {}
            futures::future::Either::Right((_event, _)) => {}
        }

        if let (Some(dialer), Some(listener)) = (dialer_connection_id, listener_connection_id) {
            return (dialer, listener);
        }
    }
}

#[tokio::test]
async fn open_stream_rejects_unknown_connection_id() {
    let swarm = Swarm::new_ephemeral_tokio(|_| Behaviour::new(shared_allow_list([])));
    let mut control = swarm.behaviour().new_control();

    let error = control
        .open_stream_by_id(ConnectionId::new_unchecked(999), PROTOCOL)
        .await
        .unwrap_err();

    assert!(matches!(error, OpenStreamError::ConnectionNotFound(_)));
}

#[test]
fn unlisten_allows_immediate_reregister_for_same_protocol() {
    let behaviour = Behaviour::new_allow_all();
    let mut control = behaviour.new_control();

    let incoming = control.listen(PROTOCOL).unwrap();
    assert!(control.listen(PROTOCOL).is_err());

    assert!(control.unlisten(&PROTOCOL));

    let _replacement = control.listen(PROTOCOL).unwrap();
    drop(incoming);
}

#[tokio::test]
async fn open_stream_uses_explicit_connection_and_reports_listener_connection_id() {
    let mut swarm1 = Swarm::new_ephemeral_tokio(|_| Behaviour::new(shared_allow_list([])));
    let swarm1_peer_id = *swarm1.local_peer_id();
    let mut swarm2 =
        Swarm::new_ephemeral_tokio(|_| Behaviour::new(shared_allow_list([swarm1_peer_id])));

    let mut control1 = swarm1.behaviour().new_control();
    let mut incoming = swarm2.behaviour().new_control().listen(PROTOCOL).unwrap();

    swarm2.listen().with_memory_addr_external().await;
    let (dialer_connection_id, listener_connection_id) =
        connect_and_get_connection_ids(&mut swarm1, &mut swarm2).await;

    let listener = tokio::spawn(async move {
        let mut incoming_stream = incoming.next().await.unwrap();
        let observed_connection_id = incoming_stream.connection_id;
        let mut buf = [0u8; 1];
        incoming_stream.stream.read_exact(&mut buf).await.unwrap();
        incoming_stream.stream.write_all(&buf).await.unwrap();
        incoming_stream.stream.close().await.unwrap();
        observed_connection_id
    });

    tokio::spawn(swarm1.loop_on_next());
    tokio::spawn(swarm2.loop_on_next());

    let mut stream = control1
        .open_stream_by_id(dialer_connection_id, PROTOCOL)
        .await
        .unwrap();
    stream.write_all(&[7]).await.unwrap();
    let mut buf = [0u8; 1];
    stream.read_exact(&mut buf).await.unwrap();
    assert_eq!(buf, [7]);

    assert_eq!(listener.await.unwrap(), listener_connection_id);
}

#[tokio::test]
async fn open_stream_allows_listener_side_connection_id() {
    let mut swarm2 = Swarm::new_ephemeral_tokio(|_| Behaviour::new(shared_allow_list([])));
    let swarm2_peer_id = *swarm2.local_peer_id();
    let mut swarm1 =
        Swarm::new_ephemeral_tokio(|_| Behaviour::new(shared_allow_list([swarm2_peer_id])));

    let mut incoming = swarm1.behaviour().new_control().listen(PROTOCOL).unwrap();
    let mut control2 = swarm2.behaviour().new_control();

    swarm2.listen().with_memory_addr_external().await;
    let (dialer_connection_id, listener_connection_id) =
        connect_and_get_connection_ids(&mut swarm1, &mut swarm2).await;

    let dialer = tokio::spawn(async move {
        let mut incoming_stream = incoming.next().await.unwrap();
        let observed_connection_id = incoming_stream.connection_id;
        let mut buf = [0u8; 1];
        incoming_stream.stream.read_exact(&mut buf).await.unwrap();
        incoming_stream.stream.write_all(&buf).await.unwrap();
        incoming_stream.stream.close().await.unwrap();
        observed_connection_id
    });

    tokio::spawn(swarm1.loop_on_next());
    tokio::spawn(swarm2.loop_on_next());

    let mut stream = control2
        .open_stream_by_id(listener_connection_id, PROTOCOL)
        .await
        .unwrap();
    stream.write_all(&[9]).await.unwrap();
    let mut buf = [0u8; 1];
    stream.read_exact(&mut buf).await.unwrap();
    assert_eq!(buf, [9]);

    assert_eq!(dialer.await.unwrap(), dialer_connection_id);
}

#[tokio::test]
async fn peer_not_in_global_allow_list_is_rejected_before_incoming_stream_is_exposed() {
    let mut swarm1 = Swarm::new_ephemeral_tokio(|_| Behaviour::new(shared_allow_list([])));
    let mut swarm2 = Swarm::new_ephemeral_tokio(|_| Behaviour::new(shared_allow_list([])));

    let mut control1 = swarm1.behaviour().new_control();
    let mut incoming = swarm2.behaviour().new_control().listen(PROTOCOL).unwrap();

    swarm2.listen().with_memory_addr_external().await;
    let (dialer_connection_id, _) = connect_and_get_connection_ids(&mut swarm1, &mut swarm2).await;

    tokio::spawn(swarm1.loop_on_next());
    tokio::spawn(swarm2.loop_on_next());

    let stream_result = tokio::time::timeout(
        Duration::from_secs(2),
        control1.open_stream_by_id(dialer_connection_id, PROTOCOL),
    )
    .await;

    if let Ok(Ok(mut stream)) = stream_result {
        let _ = stream.write_all(b"x").await;
    }

    let next = tokio::time::timeout(Duration::from_millis(250), incoming.next()).await;
    assert!(
        next.is_err(),
        "unauthorized inbound stream should not reach the listener"
    );
}

#[tokio::test]
async fn protocol_allow_list_can_be_stricter_than_global_allow_list() {
    let mut swarm1 = Swarm::new_ephemeral_tokio(|_| Behaviour::new(shared_allow_list([])));
    let swarm1_peer_id = *swarm1.local_peer_id();
    let allow_all = shared_allow_list([swarm1_peer_id]);
    let stricter_protocol_allow_list = shared_allow_list([]);
    let mut swarm2 = Swarm::new_ephemeral_tokio(|_| Behaviour::new(allow_all));

    let mut control1 = swarm1.behaviour().new_control();
    let mut incoming = swarm2
        .behaviour()
        .new_control()
        .listen_with_allow_list(
            PROTOCOL,
            ProtocolAllowList::peer_set(stricter_protocol_allow_list),
        )
        .unwrap();

    swarm2.listen().with_memory_addr_external().await;
    let (dialer_connection_id, _) = connect_and_get_connection_ids(&mut swarm1, &mut swarm2).await;

    tokio::spawn(swarm1.loop_on_next());
    tokio::spawn(swarm2.loop_on_next());

    let stream_result = tokio::time::timeout(
        Duration::from_secs(2),
        control1.open_stream_by_id(dialer_connection_id, PROTOCOL),
    )
    .await;

    if let Ok(Ok(mut stream)) = stream_result {
        let _ = stream.write_all(b"x").await;
    }

    let next = tokio::time::timeout(Duration::from_millis(250), incoming.next()).await;
    assert!(
        next.is_err(),
        "protocol-local allow list should block the peer"
    );
}

#[tokio::test]
async fn listen_defaults_to_inheriting_the_global_allow_list() {
    let mut swarm1 = Swarm::new_ephemeral_tokio(|_| Behaviour::new(shared_allow_list([])));
    let swarm1_peer_id = *swarm1.local_peer_id();
    let global_allow_list = shared_allow_list([swarm1_peer_id]);
    let mut swarm2 = Swarm::new_ephemeral_tokio(|_| Behaviour::new(global_allow_list));

    let mut control1 = swarm1.behaviour().new_control();
    let mut incoming = swarm2.behaviour().new_control().listen(PROTOCOL).unwrap();

    swarm2.listen().with_memory_addr_external().await;
    let (dialer_connection_id, _) = connect_and_get_connection_ids(&mut swarm1, &mut swarm2).await;

    let listener = tokio::spawn(async move {
        let mut incoming_stream = incoming.next().await.unwrap();
        let mut buf = [0u8; 1];
        incoming_stream.stream.read_exact(&mut buf).await.unwrap();
        incoming_stream.stream.write_all(&buf).await.unwrap();
    });

    tokio::spawn(swarm1.loop_on_next());
    tokio::spawn(swarm2.loop_on_next());

    let mut stream = control1
        .open_stream_by_id(dialer_connection_id, PROTOCOL)
        .await
        .unwrap();
    stream.write_all(&[3]).await.unwrap();
    let mut buf = [0u8; 1];
    stream.read_exact(&mut buf).await.unwrap();
    assert_eq!(buf, [3]);

    listener.await.unwrap();
}

#[tokio::test]
async fn allow_all_behaviour_accepts_inbound_without_explicit_peer_allow_list() {
    let mut swarm1 = Swarm::new_ephemeral_tokio(|_| Behaviour::new(shared_allow_list([])));
    let mut swarm2 = Swarm::new_ephemeral_tokio(|_| Behaviour::new_allow_all());

    let mut control1 = swarm1.behaviour().new_control();
    let mut incoming = swarm2.behaviour().new_control().listen(PROTOCOL).unwrap();

    swarm2.listen().with_memory_addr_external().await;
    let (dialer_connection_id, _) = connect_and_get_connection_ids(&mut swarm1, &mut swarm2).await;

    let listener = tokio::spawn(async move {
        let mut incoming_stream = incoming.next().await.unwrap();
        let mut buf = [0u8; 1];
        incoming_stream.stream.read_exact(&mut buf).await.unwrap();
        incoming_stream.stream.write_all(&buf).await.unwrap();
    });

    tokio::spawn(swarm1.loop_on_next());
    tokio::spawn(swarm2.loop_on_next());

    let mut stream = control1
        .open_stream_by_id(dialer_connection_id, PROTOCOL)
        .await
        .unwrap();
    stream.write_all(&[9]).await.unwrap();
    let mut buf = [0u8; 1];
    stream.read_exact(&mut buf).await.unwrap();
    assert_eq!(buf, [9]);

    listener.await.unwrap();
}
