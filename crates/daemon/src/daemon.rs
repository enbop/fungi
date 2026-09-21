use std::{
    collections::BTreeSet,
    env,
    net::{Ipv4Addr, Ipv6Addr},
    path::PathBuf,
    time::Duration,
};

use crate::{
    Connectivity, DaemonArgs, InboundAccessPolicy, Settings,
    control::{FungiControl, FungiControlInit},
    controls::{
        NodeCapabilitiesControl, ServiceControlProtocolControl, ServiceDiscoveryControl,
        TcpTunnelingControl, mdns::MdnsControl,
    },
    runtime::{RuntimeControl, wasmtime_runtime_supported},
};
use anyhow::{Result, bail};
use fungi_config::{
    FungiConfig,
    devices::{DeviceInfo, DevicesConfig},
    direct_addresses::DirectAddressCache,
    trusted_devices::TrustedDevicesConfig,
};
use fungi_swarm::{FungiSwarm, PeerAddressSource, State, TSwarm};
use fungi_util::keypair::get_keypair_from_dir;
use libp2p::{Multiaddr, identity::Keypair, multiaddr::Protocol};
use tokio::task::JoinHandle;

use crate::{
    devices::{Devices, DevicesInit},
    service_accesses::ServiceAccesses,
    services::{Services, ServicesInit},
};

/// Owns the lifecycle of one running daemon instance.
///
/// Shared application APIs live in [`FungiControl`]; this non-cloneable root retains the unique
/// background tasks and aborts them when dropped.
pub struct FungiDaemon {
    control: FungiControl,
    swarm_task: Option<JoinHandle<()>>,
    direct_address_cache_sync_task: Option<JoinHandle<()>>,
}

impl FungiDaemon {
    pub fn control(&self) -> FungiControl {
        self.control.clone()
    }

    pub(crate) fn control_ref(&self) -> &FungiControl {
        &self.control
    }

    pub async fn start(fungi_dir: PathBuf, args: DaemonArgs) -> Result<Self> {
        println!("Fungi directory: {fungi_dir:?}");

        let config = FungiConfig::apply_from_dir(&fungi_dir)?;
        let keypair = get_keypair_from_dir(&fungi_dir)?;

        let devices_config = DevicesConfig::apply_from_dir(&fungi_dir)?;
        let trusted_devices_config = TrustedDevicesConfig::apply_from_dir(&fungi_dir)?;
        let direct_address_cache = DirectAddressCache::apply_from_dir(&fungi_dir)?;

        Self::start_with(
            args,
            config,
            keypair,
            devices_config,
            trusted_devices_config,
            direct_address_cache,
        )
        .await
    }

    pub async fn start_with(
        _args: DaemonArgs,
        config: FungiConfig,
        keypair: Keypair,
        devices_config: DevicesConfig,
        trusted_devices_config: TrustedDevicesConfig,
        direct_address_cache: DirectAddressCache,
    ) -> Result<Self> {
        let state = State::new(
            trusted_devices_config
                .trusted_devices
                .clone()
                .into_iter()
                .collect(),
        );
        let inbound_access =
            InboundAccessPolicy::new(trusted_devices_config, state.incoming_allowed_peers());
        hydrate_device_addresses(&state, &devices_config);
        hydrate_direct_address_cache(&state, &direct_address_cache);

        let relay_addrs = config
            .network
            .effective_relay_addresses(&fungi_swarm::get_default_relay_addrs())
            .into_iter()
            .map(|entry| entry.address)
            .collect::<Vec<_>>();
        if relay_addrs.is_empty() {
            log::info!("Run without relay addresses");
        } else {
            for addr in &relay_addrs {
                log::info!("Using relay address: {addr}");
            }
        }

        let idle_connection_timeout =
            Duration::from_secs(config.network.idle_connection_timeout_secs.max(30));

        let (swarm_control, swarm_task) = FungiSwarm::start_swarm(
            keypair,
            state.clone(),
            relay_addrs,
            idle_connection_timeout,
            |swarm| {
                apply_listen(swarm, &config).expect("failed to configure swarm listeners");
            },
        )
        .await?;
        let mdns_control = MdnsControl::new();
        // TODO duplicate with libp2p-mdns?
        let device_info = mdns_device_info(&config, swarm_control.local_peer_id());
        mdns_control.start(device_info.clone(), state.clone())?;
        let connectivity =
            Connectivity::new(swarm_control.clone(), mdns_control, direct_address_cache);

        let fungi_home = config
            .config_file_path()
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .to_path_buf();
        let settings = Settings::new(config.clone());
        let runtime_root = config
            .config_file_path()
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join("runtime");
        let runtime_control = RuntimeControl::new(
            runtime_root,
            env::current_exe()
                .map_err(|e| anyhow::anyhow!("Failed to resolve current executable: {e}"))?,
            fungi_home.clone(),
            fungi_home.join("services"),
            config.runtime.allowed_host_paths.clone(),
            config.runtime.wasmtime_enabled() && wasmtime_runtime_supported(),
        )?;
        runtime_control.restore_persisted_state().await?;
        let service_discovery_control =
            ServiceDiscoveryControl::new(swarm_control.clone(), runtime_control.clone());
        service_discovery_control.start()?;
        let node_capabilities_control = NodeCapabilitiesControl::new(
            swarm_control.clone(),
            settings.config_handle(),
            runtime_control.clone(),
        );
        node_capabilities_control.start()?;

        let tcp_tunneling_control = TcpTunnelingControl::new(swarm_control.clone());

        let service_control_protocol_control = ServiceControlProtocolControl::new(
            swarm_control.clone(),
            fungi_home.clone(),
            runtime_control.clone(),
            tcp_tunneling_control.clone(),
        );
        service_control_protocol_control.start()?;

        let service_access =
            ServiceAccesses::new(fungi_home.clone(), tcp_tunneling_control.clone());
        let services = Services::new(ServicesInit {
            local_device_id: device_info.peer_id,
            fungi_dir: fungi_home,
            runtime: runtime_control,
            service_discovery: service_discovery_control,
            service_control: service_control_protocol_control,
            tcp_tunneling: tcp_tunneling_control,
        });
        let devices = Devices::new(DevicesInit {
            local_device: device_info,
            config: devices_config,
            connectivity: connectivity.clone(),
            services: services.clone(),
            node_capabilities: node_capabilities_control,
        });

        let direct_address_cache_sync_task = connectivity.spawn_direct_address_cache_sync();
        let control = FungiControl::new(FungiControlInit {
            settings,
            devices,
            services,
            service_access,
            inbound_access,
            connectivity,
        });

        restore_local_service_endpoint_listeners(&control).await?;

        Ok(Self {
            control,
            swarm_task: Some(swarm_task),
            direct_address_cache_sync_task: Some(direct_address_cache_sync_task),
        })
    }

    pub async fn wait(&mut self) -> Result<()> {
        let result = await_task_once(&mut self.swarm_task)
            .await
            .ok_or_else(|| anyhow::anyhow!("swarm task has already been joined"))?;
        match result {
            Ok(()) => bail!("swarm task stopped unexpectedly"),
            Err(error) => Err(anyhow::anyhow!("swarm task failed: {error}")),
        }
    }

    pub async fn shutdown(mut self) {
        abort_and_join_task(&mut self.swarm_task).await;
        abort_and_join_task(&mut self.direct_address_cache_sync_task).await;
    }
}

impl Drop for FungiDaemon {
    fn drop(&mut self) {
        abort_task(&self.swarm_task);
        abort_task(&self.direct_address_cache_sync_task);
    }
}

/// Keeps the handle in its owner while pending, then removes it after consuming the result.
/// This remains cancellation-safe inside `tokio::select!` and prevents a second join.
async fn await_task_once(
    task: &mut Option<JoinHandle<()>>,
) -> Option<std::result::Result<(), tokio::task::JoinError>> {
    let result = task.as_mut()?.await;
    task.take();
    Some(result)
}

async fn abort_and_join_task(task: &mut Option<JoinHandle<()>>) {
    if let Some(task) = task.take() {
        task.abort();
        let _ = task.await;
    }
}

fn abort_task(task: &Option<JoinHandle<()>>) {
    if let Some(task) = task {
        task.abort();
    }
}

/// Restores listeners for services running on this device. This path performs no remote refresh.
async fn restore_local_service_endpoint_listeners(control: &FungiControl) -> Result<()> {
    let mut listening_rules = control.services().tcp_tunneling().get_listening_rules();
    let mut restored_protocols = std::collections::BTreeSet::new();

    for service in control.services().runtime().list_services().await? {
        if !service.status.is_running() {
            continue;
        }

        for endpoint in service.exposed_endpoints {
            restore_service_endpoint_listener(
                control.services().tcp_tunneling(),
                &mut listening_rules,
                &mut restored_protocols,
                endpoint.host_port,
                endpoint.protocol,
            )
            .await;
        }
    }

    for manifest in control
        .services()
        .runtime()
        .desired_running_service_manifests()
    {
        for endpoint in crate::runtime::service_expose_endpoint_bindings(&manifest) {
            restore_service_endpoint_listener(
                control.services().tcp_tunneling(),
                &mut listening_rules,
                &mut restored_protocols,
                endpoint.host_port,
                endpoint.protocol,
            )
            .await;
        }
    }

    Ok(())
}

fn hydrate_device_addresses(state: &State, devices_config: &DevicesConfig) {
    let mut loaded = 0usize;
    let mut ignored = 0usize;

    for device in &devices_config.devices {
        for address in &device.multiaddrs {
            match address.parse::<Multiaddr>() {
                Ok(multiaddr) => {
                    match state.record_peer_address(
                        device.peer_id,
                        multiaddr,
                        PeerAddressSource::DeviceConfig,
                    ) {
                        fungi_swarm::PeerAddressObservation::New
                        | fungi_swarm::PeerAddressObservation::Refreshed => loaded += 1,
                        fungi_swarm::PeerAddressObservation::Ignored => ignored += 1,
                    }
                }
                Err(error) => {
                    ignored += 1;
                    log::debug!(
                        "Ignoring invalid device multiaddr for peer {}: {} ({})",
                        device.peer_id,
                        address,
                        error
                    );
                }
            }
        }
    }

    if loaded > 0 || ignored > 0 {
        log::info!(
            "Loaded {} device address(es) into dial planner state (ignored={})",
            loaded,
            ignored
        );
    }
}

fn hydrate_direct_address_cache(state: &State, cache: &DirectAddressCache) {
    let mut loaded = 0usize;
    let mut ignored = 0usize;

    for device in &cache.devices {
        let Ok(peer_id) = device.peer_id.parse::<libp2p::PeerId>() else {
            ignored += device.addresses.len();
            continue;
        };

        for entry in &device.addresses {
            match entry.address.parse::<Multiaddr>() {
                Ok(multiaddr) => {
                    match state.restore_peer_address_record(
                        peer_id,
                        multiaddr,
                        PeerAddressSource::DirectCache,
                        entry.first_success_at,
                        entry.last_success_at,
                        entry.success_count,
                    ) {
                        fungi_swarm::PeerAddressObservation::New
                        | fungi_swarm::PeerAddressObservation::Refreshed => loaded += 1,
                        fungi_swarm::PeerAddressObservation::Ignored => ignored += 1,
                    }
                }
                Err(error) => {
                    ignored += 1;
                    log::debug!(
                        "Ignoring invalid cached direct address for peer {}: {} ({})",
                        device.peer_id,
                        entry.address,
                        error
                    );
                }
            }
        }
    }

    if loaded > 0 || ignored > 0 {
        log::info!(
            "Loaded {} cached direct address(es) into dial planner state (ignored={})",
            loaded,
            ignored
        );
    }
}

fn mdns_device_info(config: &FungiConfig, peer_id: libp2p::PeerId) -> DeviceInfo {
    let mut device_info = DeviceInfo::this_device(peer_id, config.get_hostname());

    for ip in &device_info.private_ips {
        let ip_version = if ip.contains(':') { "6" } else { "4" };
        if config.network.listen_tcp_port != 0 {
            device_info.multiaddrs.push(format!(
                "/ip{ip_version}/{ip}/tcp/{}/p2p/{peer_id}",
                config.network.listen_tcp_port
            ));
        }

        if config.network.listen_udp_port != 0 {
            device_info.multiaddrs.push(format!(
                "/ip{ip_version}/{ip}/udp/{}/quic-v1/p2p/{peer_id}",
                config.network.listen_udp_port
            ));
        }
    }

    device_info
}

async fn restore_service_endpoint_listener(
    tcp_tunneling_control: &TcpTunnelingControl,
    listening_rules: &mut Vec<(String, fungi_config::tcp_tunneling::ListeningRule)>,
    restored_protocols: &mut BTreeSet<String>,
    host_port: u16,
    protocol: String,
) {
    let protocol_key = protocol.clone();
    let already_present = listening_rules.iter().any(|(_, rule)| {
        rule.host == "127.0.0.1"
            && rule.port == host_port
            && rule.protocol.as_deref() == Some(protocol_key.as_str())
    });
    if already_present || !restored_protocols.insert(format!("{host_port}:{protocol_key}")) {
        return;
    }

    let rule = fungi_config::tcp_tunneling::ListeningRule {
        host: "127.0.0.1".to_string(),
        port: host_port,
        protocol: Some(protocol),
    };

    match tcp_tunneling_control.add_listening_rule(rule.clone()).await {
        Ok(rule_id) => listening_rules.push((rule_id, rule)),
        Err(error) => {
            log::warn!(
                "Failed to restore service endpoint listener on 127.0.0.1:{}: {}",
                host_port,
                error
            );
        }
    }
}

fn apply_listen(swarm: &mut TSwarm, config: &FungiConfig) -> Result<()> {
    let tcp_addrs = [
        Multiaddr::empty()
            .with(Protocol::from(Ipv4Addr::UNSPECIFIED))
            .with(Protocol::Tcp(config.network.listen_tcp_port)),
        Multiaddr::empty()
            .with(Protocol::from(Ipv6Addr::UNSPECIFIED))
            .with(Protocol::Tcp(config.network.listen_tcp_port)),
    ];
    let quic_addrs = [
        Multiaddr::empty()
            .with(Protocol::from(Ipv6Addr::UNSPECIFIED))
            .with(Protocol::Udp(config.network.listen_udp_port))
            .with(Protocol::QuicV1),
        Multiaddr::empty()
            .with(Protocol::from(Ipv4Addr::UNSPECIFIED))
            .with(Protocol::Udp(config.network.listen_udp_port))
            .with(Protocol::QuicV1),
    ];

    let mut tcp_listening = false;
    let mut tcp_errors = Vec::new();
    for addr in tcp_addrs {
        match swarm.listen_on(addr.clone()) {
            Ok(_) => tcp_listening = true,
            Err(error) => {
                log::warn!("Failed to listen on {addr}: {error}");
                tcp_errors.push(format!("{addr}: {error}"));
            }
        }
    }

    if !tcp_listening {
        bail!(
            "Failed to open any TCP listen address: {}",
            tcp_errors.join("; ")
        );
    }

    let mut quic_listening = false;
    let mut quic_errors = Vec::new();
    for addr in quic_addrs {
        match swarm.listen_on(addr.clone()) {
            Ok(_) => quic_listening = true,
            Err(error) => {
                log::warn!("Failed to listen on {addr}: {error}");
                quic_errors.push(format!("{addr}: {error}"));
            }
        }
    }

    if !quic_listening && !quic_errors.is_empty() {
        log::warn!(
            "No QUIC listen address could be opened; continuing with TCP only: {}",
            quic_errors.join("; ")
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn completed_task_is_removed_after_its_result_is_joined() {
        let mut task = Some(tokio::spawn(async {}));

        let result = await_task_once(&mut task).await.unwrap();

        assert!(result.is_ok());
        assert!(task.is_none());
        abort_and_join_task(&mut task).await;
    }

    #[tokio::test]
    async fn cancelling_task_wait_preserves_daemon_ownership() {
        let mut task = Some(tokio::spawn(std::future::pending::<()>()));
        let mut wait = Box::pin(await_task_once(&mut task));

        assert!(futures::poll!(&mut wait).is_pending());
        drop(wait);

        assert!(task.is_some());
        abort_and_join_task(&mut task).await;
        assert!(task.is_none());
    }
}
