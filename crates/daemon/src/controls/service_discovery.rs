use std::{collections::HashSet, sync::Arc, time::Duration};

use crate::{CatalogService, RuntimeControl};
use anyhow::Result;
use fungi_swarm::{ConnectionSelectionStrategy, SwarmControl};
use fungi_util::protocols::FUNGI_SERVICE_DISCOVERY_PROTOCOL;
use futures::StreamExt;
use libp2p::{
    PeerId,
    futures::{AsyncReadExt, AsyncWriteExt},
};
use libp2p_stream::IncomingStreams;
use parking_lot::RwLock;

#[derive(Clone)]
pub struct ServiceDiscoveryControl {
    swarm_control: SwarmControl,
    runtime_control: RuntimeControl,
    incoming_allowed_peers: Arc<RwLock<HashSet<PeerId>>>,
}

impl ServiceDiscoveryControl {
    const CONNECT_SNIFF_WAIT: Duration = Duration::from_secs(3);

    pub fn new(
        swarm_control: SwarmControl,
        runtime_control: RuntimeControl,
        incoming_allowed_peers: Arc<RwLock<HashSet<PeerId>>>,
    ) -> Self {
        Self {
            swarm_control,
            runtime_control,
            incoming_allowed_peers,
        }
    }

    pub fn start(&self) -> Result<()> {
        let incoming_streams = self
            .swarm_control
            .accept_incoming_streams(FUNGI_SERVICE_DISCOVERY_PROTOCOL)
            .map_err(anyhow::Error::from)?;
        let this = self.clone();
        tokio::spawn(async move {
            this.listen_from_libp2p_stream(incoming_streams).await;
        });
        Ok(())
    }

    pub async fn list_peer_catalog(&self, peer_id: PeerId) -> Result<Vec<CatalogService>> {
        let (mut stream, _handle, _connection_id) = self
            .swarm_control
            .open_stream_with_strategy(
                peer_id,
                FUNGI_SERVICE_DISCOVERY_PROTOCOL,
                ConnectionSelectionStrategy::PreferDirect,
                Self::CONNECT_SNIFF_WAIT,
            )
            .await
            .map_err(|e| {
                anyhow::anyhow!("Failed to open discovery stream to peer {peer_id}: {e}")
            })?;

        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).await.map_err(|e| {
            anyhow::anyhow!("Failed to read discovery response from peer {peer_id}: {e}")
        })?;

        let services = serde_json::from_slice(&raw).map_err(|e| {
            anyhow::anyhow!("Failed to decode discovery response from peer {peer_id}: {e}")
        })?;
        Ok(services)
    }

    async fn listen_from_libp2p_stream(self, mut incoming_streams: IncomingStreams) {
        while let Some((peer_id, mut stream)) = incoming_streams.next().await {
            if !self.incoming_allowed_peers.read().contains(&peer_id) {
                log::warn!("Deny service discovery from disallowed peer: {peer_id}");
                continue;
            }

            let this = self.clone();
            tokio::spawn(async move {
                let services = match this.runtime_control.list_catalog_services().await {
                    Ok(services) => services,
                    Err(error) => {
                        log::warn!("Failed to list exposed services for discovery: {}", error);
                        Vec::new()
                    }
                };

                let payload = match serde_json::to_vec(&services) {
                    Ok(payload) => payload,
                    Err(error) => {
                        log::warn!("Failed to serialize discovery response: {}", error);
                        return;
                    }
                };

                if let Err(error) = stream.write_all(&payload).await {
                    log::warn!(
                        "Failed to write discovery response to peer {}: {}",
                        peer_id,
                        error
                    );
                    return;
                }
                let _ = stream.close().await;
            });
        }
    }
}
