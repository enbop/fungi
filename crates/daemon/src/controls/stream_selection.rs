use std::time::Duration;

use anyhow::bail;
use fungi_swarm::{ConnectionSelectionStrategy, StreamObservationHandle, SwarmControl};
use libp2p::{PeerId, Stream, StreamProtocol};
use libp2p_stream::Control;

pub(crate) async fn open_stream_with_strategy(
    swarm_control: &SwarmControl,
    stream_control: &mut Control,
    target_peer: PeerId,
    target_protocol: StreamProtocol,
    strategy: ConnectionSelectionStrategy,
    sniff_wait: Duration,
) -> anyhow::Result<(Stream, StreamObservationHandle)> {
    let candidates = swarm_control
        .connect_with_strategy(target_peer, strategy, sniff_wait)
        .await?;

    let mut last_error = None;
    for selected in &candidates {
        match stream_control
            .open_stream_on_connection(target_peer, selected.connection_id, target_protocol.clone())
            .await
        {
            Ok(stream) => {
                let stream_observation_handle = swarm_control.state().track_outbound_stream_opened(
                    target_peer,
                    selected.connection_id,
                    target_protocol.to_string(),
                );
                return Ok((stream, stream_observation_handle));
            }
            Err(e) => {
                log::warn!(
                    "Failed to open stream on connection {} to peer {} (relay={}, addr={}): {}",
                    selected.connection_id,
                    target_peer,
                    selected.is_relay,
                    selected.remote_addr,
                    e
                );
                last_error = Some(e);
            }
        }
    }

    let detail = last_error
        .map(|e| e.to_string())
        .unwrap_or_else(|| "no candidate connections returned".to_string());
    bail!(
        "Failed to open stream to peer {} using selected connections: {}",
        target_peer,
        detail
    )
}
