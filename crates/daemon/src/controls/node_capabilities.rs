use std::{collections::HashSet, sync::Arc, time::Duration};

use anyhow::Result;
use fungi_config::FungiConfig;
use fungi_stream::IncomingStreams;
use fungi_swarm::{ConnectionSelectionStrategy, SwarmControl};
use fungi_util::protocols::FUNGI_NODE_CAPABILITIES_PROTOCOL;
use futures::StreamExt;
use libp2p::{
    PeerId,
    futures::{AsyncReadExt, AsyncWriteExt},
};
use parking_lot::{Mutex, RwLock};

use crate::{NodeCapabilities, RuntimeControl, build_local_node_capabilities};

#[derive(Clone)]
pub struct NodeCapabilitiesControl {
    swarm_control: SwarmControl,
    config: Arc<Mutex<FungiConfig>>,
    runtime_control: RuntimeControl,
    incoming_allowed_peers: Arc<RwLock<HashSet<PeerId>>>,
}

impl NodeCapabilitiesControl {
    const CONNECT_SNIFF_WAIT: Duration = Duration::from_secs(3);

    pub fn new(
        swarm_control: SwarmControl,
        config: Arc<Mutex<FungiConfig>>,
        runtime_control: RuntimeControl,
        incoming_allowed_peers: Arc<RwLock<HashSet<PeerId>>>,
    ) -> Self {
        Self {
            swarm_control,
            config,
            runtime_control,
            incoming_allowed_peers,
        }
    }

    pub fn start(&self) -> Result<()> {
        let incoming_streams = self
            .swarm_control
            .accept_incoming_streams(FUNGI_NODE_CAPABILITIES_PROTOCOL)
            .map_err(anyhow::Error::from)?;
        let this = self.clone();
        tokio::spawn(async move {
            this.listen_from_incoming_streams(incoming_streams).await;
        });
        Ok(())
    }

    pub fn local_capabilities(&self) -> NodeCapabilities {
        let config = self.config.lock().clone();
        build_local_node_capabilities(&config, &self.runtime_control)
    }

    pub async fn discover_peer_capabilities(&self, peer_id: PeerId) -> Result<NodeCapabilities> {
        let (mut stream, _handle, _connection_id) = self
            .swarm_control
            .open_stream_with_strategy(
                peer_id,
                FUNGI_NODE_CAPABILITIES_PROTOCOL,
                ConnectionSelectionStrategy::PreferDirect,
                Self::CONNECT_SNIFF_WAIT,
            )
            .await
            .map_err(|e| {
                anyhow::anyhow!("Failed to open node-capabilities stream to peer {peer_id}: {e}")
            })?;

        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).await.map_err(|e| {
            anyhow::anyhow!("Failed to read node capabilities from peer {peer_id}: {e}")
        })?;

        let capabilities = serde_json::from_slice(&raw).map_err(|e| {
            anyhow::anyhow!("Failed to decode node capabilities from peer {peer_id}: {e}")
        })?;
        Ok(capabilities)
    }

    async fn listen_from_incoming_streams(self, mut incoming_streams: IncomingStreams) {
        while let Some(incoming_stream) = incoming_streams.next().await {
            let peer_id = incoming_stream.peer_id;
            let mut stream = incoming_stream.stream;
            if !self.incoming_allowed_peers.read().contains(&peer_id) {
                log::warn!("Deny node capability discovery from disallowed peer: {peer_id}");
                continue;
            }

            let this = self.clone();
            tokio::spawn(async move {
                let payload = match serde_json::to_vec(&this.local_capabilities()) {
                    Ok(payload) => payload,
                    Err(error) => {
                        log::warn!("Failed to serialize node capabilities response: {}", error);
                        return;
                    }
                };

                if let Err(error) = stream.write_all(&payload).await {
                    log::warn!(
                        "Failed to write node capabilities to peer {}: {}",
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
