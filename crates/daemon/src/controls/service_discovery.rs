use crate::{CatalogService, RuntimeControl};
use anyhow::Result;
use fungi_stream::IncomingStreams;
use fungi_swarm::SwarmControl;
use fungi_util::protocols::FUNGI_SERVICE_DISCOVERY_PROTOCOL;
use futures::StreamExt;
use libp2p::{
    PeerId,
    futures::{AsyncReadExt, AsyncWriteExt},
};

#[derive(Clone)]
pub struct ServiceDiscoveryControl {
    swarm_control: SwarmControl,
    runtime_control: RuntimeControl,
}

impl ServiceDiscoveryControl {
    pub fn new(swarm_control: SwarmControl, runtime_control: RuntimeControl) -> Self {
        Self {
            swarm_control,
            runtime_control,
        }
    }

    pub fn start(&self) -> Result<()> {
        let incoming_streams = self
            .swarm_control
            .accept_incoming_streams(FUNGI_SERVICE_DISCOVERY_PROTOCOL)
            .map_err(anyhow::Error::from)?;
        let this = self.clone();
        tokio::spawn(async move {
            this.listen_from_incoming_streams(incoming_streams).await;
        });
        Ok(())
    }

    pub async fn list_peer_services(&self, peer_id: PeerId) -> Result<Vec<CatalogService>> {
        let (mut stream, _handle, _connection_id) = self
            .swarm_control
            .open_stream(peer_id, FUNGI_SERVICE_DISCOVERY_PROTOCOL)
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

    pub async fn list_peer_catalog(&self, peer_id: PeerId) -> Result<Vec<CatalogService>> {
        self.list_peer_services(peer_id).await
    }

    async fn listen_from_incoming_streams(self, mut incoming_streams: IncomingStreams) {
        while let Some(incoming_stream) = incoming_streams.next().await {
            let peer_id = incoming_stream.peer_id;
            let mut stream = incoming_stream.stream;

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
