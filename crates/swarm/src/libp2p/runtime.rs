use super::{
    SwarmAsyncCall, SwarmControl, TSwarm,
    governance::connection_governance_loop,
    relay::{
        RefreshThrottle, RelayPeers, handle_expired_listen_addr, handle_listener_closed,
        handle_new_listen_addr, handle_relay_behaviour_event, handle_relay_refresh_behaviour_event,
        record_relay_connection_closed, record_relay_connection_established, relay_management_loop,
    },
};
use crate::{
    ExternalAddressSource, State,
    behaviours::{FungiBehaviours, FungiBehavioursEvent},
    ping::{PingRttEvent, PingState},
    state,
};
use anyhow::Result;
use libp2p::{
    Multiaddr,
    futures::StreamExt,
    identify,
    identity::Keypair,
    mdns, noise,
    swarm::{DialError, SwarmEvent},
    tcp, yamux,
};
use std::{sync::Arc, time::Duration};
use tokio::{
    sync::mpsc::{self, UnboundedReceiver},
    task::JoinHandle,
};

pub struct FungiSwarm;

impl FungiSwarm {
    pub async fn start_swarm(
        keypair: Keypair,
        state: State,
        relay_addresses: Vec<Multiaddr>,
        idle_connection_timeout: Duration,
        apply: impl FnOnce(&mut TSwarm),
    ) -> Result<(SwarmControl, JoinHandle<()>)> {
        let mdns =
            mdns::tokio::Behaviour::new(mdns::Config::default(), keypair.public().to_peer_id())?;
        let relay_peers = RelayPeers::new(relay_addresses);
        let trusted_relay_peer_ids = relay_peers.peer_ids().to_vec();

        let mut swarm = libp2p::SwarmBuilder::with_existing_identity(keypair)
            .with_tokio()
            .with_tcp(
                tcp::Config::default(),
                noise::Config::new,
                yamux::Config::default,
            )?
            .with_quic()
            .with_relay_client(noise::Config::new, yamux::Config::default)?
            .with_behaviour(|keypair, relay| {
                FungiBehaviours::new(
                    keypair,
                    relay,
                    mdns,
                    state.clone(),
                    trusted_relay_peer_ids.clone(),
                )
            })?
            .with_swarm_config(|config| {
                config.with_idle_connection_timeout(idle_connection_timeout)
            })
            .build();

        let local_peer_id = *swarm.local_peer_id();
        let stream_control = swarm.behaviour().stream.new_control();
        let refresh_throttle = RefreshThrottle::default();

        let (ping_event_tx, ping_event_rx) = mpsc::unbounded_channel::<PingRttEvent>();
        let mut ping_state = PingState::new(Duration::from_secs(15), ping_event_tx);
        ping_state.init(stream_control.clone());
        let ping_state = Arc::new(ping_state);

        apply(&mut swarm);

        let (swarm_caller_tx, swarm_caller_rx) = mpsc::unbounded_channel::<SwarmAsyncCall>();
        let (swarm_event_tx, swarm_event_rx) =
            mpsc::unbounded_channel::<SwarmEvent<FungiBehavioursEvent>>();

        let swarm_future = swarm_loop(swarm, swarm_caller_rx, swarm_event_tx);
        let swarm_control = SwarmControl::new(
            Arc::new(local_peer_id),
            swarm_caller_tx,
            stream_control,
            refresh_throttle,
            relay_peers,
            ping_state,
            state,
        );
        let event_handle_future = handle_swarm_event(swarm_control.clone(), swarm_event_rx);
        let ping_handle_future = handle_ping_event(swarm_control.clone(), ping_event_rx);
        let relay_health_future = relay_management_loop(swarm_control.clone());
        let connection_governance_future = connection_governance_loop(swarm_control.clone());

        let join_handle = tokio::spawn(async move {
            tokio::select! {
                _ = swarm_future => {},
                _ = event_handle_future => {},
                _ = ping_handle_future => {},
                _ = relay_health_future => {},
                _ = connection_governance_future => {},
            }
        });

        Ok((swarm_control, join_handle))
    }
}

async fn swarm_loop(
    mut swarm: TSwarm,
    mut swarm_caller_rx: UnboundedReceiver<SwarmAsyncCall>,
    event_tx: mpsc::UnboundedSender<SwarmEvent<FungiBehavioursEvent>>,
) {
    loop {
        tokio::select! {
            swarm_events = swarm.select_next_some() => {
                if let Err(error) = event_tx.send(swarm_events) {
                    log::error!("Failed to send swarm event: {error:?}");
                    break;
                }
            },
            invoke = swarm_caller_rx.recv() => {
                let Some(SwarmAsyncCall { request, response }) = invoke else {
                    log::debug!("Swarm caller channel closed");
                    break;
                };
                let result = request(&mut swarm);
                response.complete(result);
            }
        }
    }
    log::info!("Swarm loop exited");
}

async fn handle_swarm_event(
    swarm_control: SwarmControl,
    mut event_rx: UnboundedReceiver<SwarmEvent<FungiBehavioursEvent>>,
) {
    loop {
        let Some(event) = event_rx.recv().await else {
            log::debug!("Swarm event channel closed");
            break;
        };
        match event {
            SwarmEvent::NewListenAddr { address, .. } => {
                println!("[Swarm event] NewListenAddr {address:?}");
                handle_new_listen_addr(&swarm_control, address);
            }
            SwarmEvent::ExpiredListenAddr { address, .. } => {
                handle_expired_listen_addr(&swarm_control, address);
            }
            SwarmEvent::ListenerClosed {
                listener_id: _,
                addresses,
                reason,
            } => {
                handle_listener_closed(&swarm_control, addresses, reason);
            }
            SwarmEvent::Behaviour(FungiBehavioursEvent::Mdns(event)) => {
                handle_mdns_behaviour_event(&swarm_control, event);
            }
            SwarmEvent::Behaviour(FungiBehavioursEvent::Identify(event)) => {
                handle_identify_behaviour_event(&swarm_control, event);
            }
            SwarmEvent::Behaviour(FungiBehavioursEvent::Relay(event)) => {
                handle_relay_behaviour_event(&swarm_control, event);
            }
            SwarmEvent::Behaviour(FungiBehavioursEvent::RelayRefresh(event)) => {
                handle_relay_refresh_behaviour_event(&swarm_control, event);
            }
            SwarmEvent::Behaviour(FungiBehavioursEvent::Dcutr(event)) => {
                handle_dcutr_behaviour_event(event);
            }
            SwarmEvent::NewExternalAddrCandidate { address, .. } => {
                swarm_control.state().record_external_address_candidate(
                    address.clone(),
                    ExternalAddressSource::SwarmCandidate,
                );
                log::info!("[Swarm event] NewExternalAddrCandidate {address:?}");
            }
            SwarmEvent::ExternalAddrConfirmed { address } => {
                swarm_control.state().record_external_address_confirmed(
                    address.clone(),
                    ExternalAddressSource::SwarmConfirmed,
                );
                log::info!("[Swarm event] ExternalAddrConfirmed {address:?}");
            }
            SwarmEvent::ExternalAddrExpired { address } => {
                swarm_control.state().expire_external_address(&address);
                log::info!("[Swarm event] ExternalAddrExpired {address:?}");
            }
            SwarmEvent::ConnectionEstablished {
                peer_id,
                connection_id,
                endpoint,
                ..
            } => {
                log::debug!(
                    "Established connection {:?} - peer_id {:?} - multiaddr {:?} - is_dialer {:?}",
                    connection_id,
                    peer_id,
                    endpoint.get_remote_address(),
                    endpoint.is_dialer()
                );

                state::handle_connection_established(
                    &swarm_control,
                    peer_id,
                    connection_id,
                    &endpoint,
                );
                record_relay_connection_established(
                    &swarm_control,
                    peer_id,
                    connection_id,
                    endpoint.get_remote_address(),
                );
            }
            SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                log::info!("[Swarm event] OutgoingConnectionError {peer_id:?}: {error:?}");
                let Some(peer_id) = peer_id else {
                    continue;
                };
                handle_outgoing_connection_error(&swarm_control, peer_id, error);
            }
            SwarmEvent::ConnectionClosed {
                peer_id,
                connection_id,
                endpoint,
                cause,
                ..
            } => {
                log::debug!(
                    "Closed connection {} - peer_id {} - multiaddr {:?} - is_dialer {:?} - cause {:?}",
                    connection_id,
                    peer_id,
                    endpoint.get_remote_address(),
                    endpoint.is_dialer(),
                    cause
                );

                record_relay_connection_closed(
                    &swarm_control,
                    peer_id,
                    connection_id,
                    endpoint.get_remote_address(),
                    cause.as_ref().map(|cause| format!("{cause:?}")),
                );
                state::handle_connection_closed(&swarm_control, peer_id, connection_id);
            }
            _ => {}
        }
    }
}

fn handle_mdns_behaviour_event(swarm_control: &SwarmControl, event: mdns::Event) {
    match event {
        mdns::Event::Discovered(entries) => {
            let mut new_count = 0usize;
            let mut refreshed_count = 0usize;
            let mut ignored_count = 0usize;

            for (peer_id, addr) in entries {
                match swarm_control.state().record_peer_address(
                    peer_id,
                    addr,
                    crate::PeerAddressSource::Mdns,
                ) {
                    crate::PeerAddressObservation::New => new_count += 1,
                    crate::PeerAddressObservation::Refreshed => refreshed_count += 1,
                    crate::PeerAddressObservation::Ignored => ignored_count += 1,
                }
            }

            if new_count > 0 || refreshed_count > 0 || ignored_count > 0 {
                log::debug!(
                    "mDNS discovery updated peer address state: new={} refreshed={} ignored={}",
                    new_count,
                    refreshed_count,
                    ignored_count
                );
            }
        }
        mdns::Event::Expired(entries) => {
            let mut expired_count = 0usize;
            let mut missing_count = 0usize;

            for (peer_id, addr) in entries {
                if swarm_control.state().expire_peer_address(peer_id, addr) {
                    expired_count += 1;
                } else {
                    missing_count += 1;
                }
            }

            if expired_count > 0 || missing_count > 0 {
                log::debug!(
                    "mDNS expiry updated peer address state: expired={} missing={}",
                    expired_count,
                    missing_count
                );
            }
        }
    }
}

fn handle_identify_behaviour_event(swarm_control: &SwarmControl, event: identify::Event) {
    match event {
        identify::Event::Received { peer_id, info, .. } => {
            let mut new_addresses = Vec::new();
            let mut refreshed_count = 0usize;
            let mut ignored_count = 0usize;

            for address in info.listen_addrs {
                match swarm_control.state().record_peer_address(
                    peer_id,
                    address.clone(),
                    crate::PeerAddressSource::Identify,
                ) {
                    crate::PeerAddressObservation::New => new_addresses.push(address),
                    crate::PeerAddressObservation::Refreshed => refreshed_count += 1,
                    crate::PeerAddressObservation::Ignored => ignored_count += 1,
                }
            }

            if !new_addresses.is_empty() {
                log::info!(
                    "Identify learned {} new address(es) for peer {}: {}",
                    new_addresses.len(),
                    peer_id,
                    summarize_multiaddrs(&new_addresses)
                );
            }

            if refreshed_count > 0 {
                log::debug!(
                    "Identify refreshed {} existing address(es) for peer {}",
                    refreshed_count,
                    peer_id
                );
            }

            if ignored_count > 0 {
                log::debug!(
                    "Identify ignored {} unusable address(es) for peer {}",
                    ignored_count,
                    peer_id
                );
            }
        }
        identify::Event::Sent { peer_id, .. } => {
            log::debug!("Identify sent to peer {}", peer_id);
        }
        identify::Event::Pushed { peer_id, .. } => {
            log::debug!("Identify pushed update to peer {}", peer_id);
        }
        identify::Event::Error { peer_id, error, .. } => {
            log::debug!("Identify error for peer {}: {}", peer_id, error);
        }
    }
}

fn handle_dcutr_behaviour_event(event: libp2p::dcutr::Event) {
    match event.result {
        Ok(connection_id) => {
            log::info!(
                "Hole punch succeeded for peer {} on connection {:?}",
                event.remote_peer_id,
                connection_id
            );
        }
        Err(error) => {
            log::warn!(
                "Hole punch failed for peer {}: {}",
                event.remote_peer_id,
                error
            );
        }
    }
}

fn summarize_multiaddrs(addrs: &[Multiaddr]) -> String {
    const PREVIEW_LIMIT: usize = 3;

    let preview = addrs
        .iter()
        .take(PREVIEW_LIMIT)
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ");

    if addrs.len() > PREVIEW_LIMIT {
        format!("{preview}, ... (+{} more)", addrs.len() - PREVIEW_LIMIT)
    } else {
        preview
    }
}

fn handle_outgoing_connection_error(
    swarm_control: &SwarmControl,
    peer_id: libp2p::PeerId,
    error: DialError,
) {
    if let Some(completer) = swarm_control
        .state()
        .dial_callback()
        .lock()
        .remove(&peer_id)
    {
        completer.complete(Err(error));
    }
}

async fn handle_ping_event(
    swarm_control: SwarmControl,
    mut ping_event_rx: UnboundedReceiver<PingRttEvent>,
) {
    loop {
        let Some(event) = ping_event_rx.recv().await else {
            log::debug!("Ping event channel closed");
            break;
        };
        swarm_control
            .state()
            .update_connection_ping(&event.connection_id, event.rtt);
    }
}
