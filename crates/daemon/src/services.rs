use std::{
    collections::{BTreeMap, HashMap},
    future::Future,
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime},
};

use anyhow::{Context as _, Result};
use fungi_config::runtime::Runtime as RuntimeConfig;
use fungi_config::service_cache::DeviceServiceSnapshotCache;
use libp2p::PeerId;
use parking_lot::Mutex;

use crate::{
    DeviceService, DeviceServiceSnapshot, ManifestResolutionPolicy, ServiceApplyOutcome,
    ServiceInstance, ServiceLogs, ServiceLogsOptions,
    controls::{
        DockerControl, ServiceControlProtocolControl, ServiceDiscoveryControl, TcpTunnelingControl,
    },
    runtime::RuntimeControl,
    service_endpoints::{
        sync_applied_service_endpoint_listeners, sync_service_endpoint_listeners_by_name,
        sync_service_endpoint_listeners_for_manifest,
    },
};

const REMOTE_SERVICE_REFRESH_TIMEOUT: Duration = Duration::from_secs(15);

struct LocalServiceBackend {
    runtime: RuntimeControl,
    docker: Option<DockerControl>,
    tcp_tunneling: TcpTunnelingControl,
}

struct RemoteServiceBackend {
    discovery: ServiceDiscoveryControl,
    control: ServiceControlProtocolControl,
}

/// Stores best-effort remote observations independently from device-directory membership.
///
/// Per-peer epochs let removal invalidate an older in-flight write without holding a lock during
/// remote I/O. A later refresh may still create a new observation for an addressable peer.
struct DeviceServiceSnapshots {
    fungi_dir: PathBuf,
    epochs: Mutex<HashMap<PeerId, u64>>,
}

impl DeviceServiceSnapshots {
    fn new(fungi_dir: PathBuf) -> Self {
        Self {
            fungi_dir,
            epochs: Mutex::new(HashMap::new()),
        }
    }

    fn get(&self, peer_id: PeerId) -> Result<Option<DeviceServiceSnapshot>> {
        let Some(snapshot_json) = self
            .cache()?
            .get_device_snapshot_json(&peer_id.to_string())?
        else {
            return Ok(None);
        };
        serde_json::from_str(&snapshot_json)
            .map(Some)
            .map_err(|error| {
                anyhow::anyhow!("failed to decode cached device service snapshot: {error}")
            })
    }

    fn epoch(&self, peer_id: PeerId) -> u64 {
        self.epochs
            .lock()
            .get(&peer_id)
            .copied()
            .unwrap_or_default()
    }

    fn save_if_current(
        &self,
        peer_id: PeerId,
        expected_epoch: u64,
        snapshot: &DeviceServiceSnapshot,
    ) -> Result<bool> {
        let epochs = self.epochs.lock();
        if epochs.get(&peer_id).copied().unwrap_or_default() != expected_epoch {
            return Ok(false);
        }

        let snapshot_json = serde_json::to_string(snapshot)?;
        self.cache()?
            .set_device_snapshot_json(snapshot.peer_id.clone(), snapshot_json)?;
        Ok(true)
    }

    fn remove(&self, peer_id: PeerId) -> Result<bool> {
        let mut epochs = self.epochs.lock();
        let epoch = epochs.entry(peer_id).or_default();
        *epoch = epoch
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("device service snapshot epoch overflow"))?;
        self.cache()?.remove_device_snapshot(&peer_id.to_string())
    }

    fn cache(&self) -> Result<DeviceServiceSnapshotCache> {
        DeviceServiceSnapshotCache::apply_from_dir(&self.fungi_dir)
    }
}

struct ServicesInner {
    local_device_id: PeerId,
    fungi_dir: PathBuf,
    snapshots: DeviceServiceSnapshots,
    local: LocalServiceBackend,
    remote: RemoteServiceBackend,
}

pub(crate) struct ServicesInit {
    pub local_device_id: PeerId,
    pub fungi_dir: PathBuf,
    pub runtime: RuntimeControl,
    pub docker: Option<DockerControl>,
    pub service_discovery: ServiceDiscoveryControl,
    pub service_control: ServiceControlProtocolControl,
    pub tcp_tunneling: TcpTunnelingControl,
}

/// Shared service domain for local and remote devices.
///
/// The same handles route local operations to runtime backends and remote operations to libp2p
/// protocols. Remote status is a cached observation rather than authoritative state.
#[derive(Clone)]
pub struct Services {
    inner: Arc<ServicesInner>,
}

impl Services {
    pub(crate) fn new(init: ServicesInit) -> Self {
        let snapshots = DeviceServiceSnapshots::new(init.fungi_dir.clone());
        Self {
            inner: Arc::new(ServicesInner {
                local_device_id: init.local_device_id,
                fungi_dir: init.fungi_dir,
                snapshots,
                local: LocalServiceBackend {
                    runtime: init.runtime,
                    docker: init.docker,
                    tcp_tunneling: init.tcp_tunneling,
                },
                remote: RemoteServiceBackend {
                    discovery: init.service_discovery,
                    control: init.service_control,
                },
            }),
        }
    }

    pub fn for_device(&self, device_id: PeerId) -> DeviceServices {
        DeviceServices {
            device_id,
            services: self.clone(),
        }
    }

    pub(crate) fn runtime(&self) -> &RuntimeControl {
        &self.inner.local.runtime
    }

    pub(crate) fn apply_runtime_config(&self, config: &RuntimeConfig) -> Result<()> {
        if let Some(docker) = &self.inner.local.docker {
            docker.update_runtime_config(config)?;
        }
        self.inner
            .local
            .runtime
            .update_allowed_host_paths(config.allowed_host_paths.clone());
        Ok(())
    }

    /// Low-level protocol handle for focused integration tests.
    pub fn service_control(&self) -> &ServiceControlProtocolControl {
        &self.inner.remote.control
    }

    pub(crate) fn tcp_tunneling(&self) -> &TcpTunnelingControl {
        &self.inner.local.tcp_tunneling
    }

    pub(crate) fn remove_snapshot(&self, peer_id: PeerId) -> Result<bool> {
        if peer_id == self.inner.local_device_id {
            return Ok(false);
        }
        self.inner.snapshots.remove(peer_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ServiceKey {
    pub device_id: PeerId,
    pub service_name: String,
}

impl ServiceKey {
    pub fn new(device_id: PeerId, service_name: impl Into<String>) -> Self {
        Self {
            device_id,
            service_name: service_name.into(),
        }
    }
}

/// Collection and observation boundary for one device's services.
#[derive(Clone)]
pub struct DeviceServices {
    device_id: PeerId,
    services: Services,
}

pub(crate) struct AppliedServiceHandle {
    pub service: ServiceHandle,
    pub outcome: Option<ServiceApplyOutcome>,
}

impl DeviceServices {
    pub fn device_id(&self) -> PeerId {
        self.device_id
    }

    fn is_local(&self) -> bool {
        self.device_id == self.services.inner.local_device_id
    }

    /// Reads the last successful remote observation without network access.
    ///
    /// Local services do not maintain a stale shadow, so this returns `None` for the local device.
    pub fn snapshot(&self) -> Result<Option<DeviceServiceSnapshot>> {
        if self.is_local() {
            return Ok(None);
        }

        self.services.inner.snapshots.get(self.device_id)
    }

    /// Observes this device now. Remote successes replace the persisted shadow; local
    /// observations are returned directly without creating a redundant cache.
    pub async fn refresh(&self) -> Result<DeviceServiceSnapshot> {
        let peer_id = self.device_id;
        let snapshot_epoch =
            (!self.is_local()).then(|| self.services.inner.snapshots.epoch(peer_id));
        let snapshot = if self.is_local() {
            let (managed, published) = tokio::try_join!(
                self.services.inner.local.runtime.list_services(),
                self.services
                    .inner
                    .local
                    .runtime
                    .list_published_device_services(),
            )?;
            merge_device_service_snapshot(peer_id, managed, published)
        } else {
            let control = self.services.inner.remote.control.clone();
            let discovery = self.services.inner.remote.discovery.clone();
            let (managed_response, published) = with_remote_refresh_timeout(async move {
                tokio::try_join!(
                    async move {
                        control.list_peer_services(peer_id).await.with_context(|| {
                            format!("failed to refresh managed services for device {peer_id}")
                        })
                    },
                    async move {
                        discovery
                            .list_peer_services(peer_id)
                            .await
                            .with_context(|| {
                                format!("failed to refresh published services for device {peer_id}")
                            })
                    },
                )
            })
            .await?;
            let managed = managed_response
                .services_json
                .as_deref()
                .map(serde_json::from_str::<Vec<ServiceInstance>>)
                .transpose()
                .map_err(|error| {
                    anyhow::anyhow!(
                        "failed to decode managed services from device {peer_id}: {error}"
                    )
                })?
                .unwrap_or_default();
            merge_device_service_snapshot(peer_id, managed, published)
        };

        if let Some(snapshot_epoch) = snapshot_epoch {
            self.services
                .inner
                .snapshots
                .save_if_current(peer_id, snapshot_epoch, &snapshot)?;
        }
        Ok(snapshot)
    }

    pub async fn list(&self) -> Result<Vec<DeviceService>> {
        Ok(self.refresh().await?.services)
    }

    /// Lists only endpoints published for connection, without the managed-service merge.
    pub async fn published(&self) -> Result<Vec<DeviceService>> {
        if self.is_local() {
            self.services
                .inner
                .local
                .runtime
                .list_published_device_services()
                .await
        } else {
            self.services
                .inner
                .remote
                .discovery
                .list_peer_services(self.device_id)
                .await
        }
    }

    pub fn service(&self, name: impl Into<String>) -> ServiceHandle {
        ServiceHandle {
            key: ServiceKey::new(self.device_id, name),
            services: self.services.clone(),
        }
    }

    pub async fn apply_manifest_yaml(
        &self,
        manifest_yaml: String,
        manifest_base_dir: Option<PathBuf>,
    ) -> Result<ServiceHandle> {
        let applied = self
            .apply_manifest_yaml_with_outcome(manifest_yaml, manifest_base_dir)
            .await?;
        if let Some(message) = applied
            .outcome
            .as_ref()
            .and_then(ServiceApplyOutcome::failure_summary)
        {
            anyhow::bail!(message);
        }
        Ok(applied.service)
    }

    pub(crate) async fn apply_manifest_yaml_with_outcome(
        &self,
        manifest_yaml: String,
        manifest_base_dir: Option<PathBuf>,
    ) -> Result<AppliedServiceHandle> {
        if self.is_local() {
            let fungi_home = self.services.inner.fungi_dir.clone();
            let base_dir = manifest_base_dir.unwrap_or_else(|| fungi_home.clone());
            let mut applied = self
                .services
                .inner
                .local
                .runtime
                .apply_manifest_yaml(
                    &manifest_yaml,
                    &base_dir,
                    &fungi_home,
                    &ManifestResolutionPolicy,
                )
                .await?;
            sync_applied_service_endpoint_listeners(
                &self.services.inner.local.runtime,
                &self.services.inner.local.tcp_tunneling,
                &mut applied,
            )
            .await;
            Ok(AppliedServiceHandle {
                service: self.service(applied.instance.name),
                outcome: Some(applied.outcome),
            })
        } else {
            let response = self
                .services
                .inner
                .remote
                .control
                .pull_peer_service(self.device_id, manifest_yaml)
                .await?;
            self.refresh_or_keep_after_mutation().await;
            let service_name = response
                .service
                .map(|service| service.name)
                .ok_or_else(|| {
                    anyhow::anyhow!("service apply response did not include a service")
                })?;
            Ok(AppliedServiceHandle {
                service: self.service(service_name),
                outcome: response.apply_outcome,
            })
        }
    }

    pub(crate) fn remove_cached_service(&self, name: &str) -> Result<bool> {
        let snapshot_epoch = self.services.inner.snapshots.epoch(self.device_id);
        let Some(mut snapshot) = self.snapshot()? else {
            return Ok(false);
        };
        let before = snapshot.services.len();
        snapshot.services.retain(|service| service.name != name);
        if snapshot.services.len() == before {
            return Ok(false);
        }
        self.services
            .inner
            .snapshots
            .save_if_current(self.device_id, snapshot_epoch, &snapshot)?;
        Ok(true)
    }

    async fn refresh_or_keep_after_mutation(&self) {
        if let Err(error) = self.refresh().await {
            log::warn!(
                "Failed to refresh device service snapshot for device {} after remote mutation: {error}",
                self.device_id
            );
        }
    }
}

/// Service identity on a device. The handle never stores a service observation, which could
/// become stale; observation and mutation always go through the owning `DeviceServices`.
#[derive(Clone)]
pub struct ServiceHandle {
    key: ServiceKey,
    services: Services,
}

impl ServiceHandle {
    pub fn key(&self) -> &ServiceKey {
        &self.key
    }

    pub fn device_id(&self) -> PeerId {
        self.key.device_id
    }

    pub fn name(&self) -> &str {
        &self.key.service_name
    }

    fn device_services(&self) -> DeviceServices {
        self.services.for_device(self.key.device_id)
    }

    fn is_local(&self) -> bool {
        self.key.device_id == self.services.inner.local_device_id
    }

    pub fn observation(&self) -> Result<Option<DeviceService>> {
        Ok(self
            .device_services()
            .snapshot()?
            .and_then(|snapshot| find_service(snapshot, &self.key.service_name)))
    }

    pub async fn inspect(&self) -> Result<DeviceService> {
        let snapshot = self.device_services().refresh().await?;
        find_service(snapshot, &self.key.service_name)
            .ok_or_else(|| anyhow::anyhow!("service not found: {}", self.key.service_name))
    }

    pub async fn start(&self) -> Result<()> {
        if self.is_local() {
            self.services
                .inner
                .local
                .runtime
                .start_by_name(&self.key.service_name)
                .await?;
            sync_service_endpoint_listeners_by_name(
                &self.services.inner.local.runtime,
                &self.services.inner.local.tcp_tunneling,
                &self.key.service_name,
                true,
            )
            .await?;
            return Ok(());
        }

        self.services
            .inner
            .remote
            .control
            .start_peer_service(self.key.device_id, self.key.service_name.clone())
            .await?;
        self.device_services()
            .refresh_or_keep_after_mutation()
            .await;
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        if self.is_local() {
            self.services
                .inner
                .local
                .runtime
                .stop_by_name(&self.key.service_name)
                .await?;
            sync_service_endpoint_listeners_by_name(
                &self.services.inner.local.runtime,
                &self.services.inner.local.tcp_tunneling,
                &self.key.service_name,
                false,
            )
            .await?;
            return Ok(());
        }

        self.services
            .inner
            .remote
            .control
            .stop_peer_service(self.key.device_id, self.key.service_name.clone())
            .await?;
        self.device_services()
            .refresh_or_keep_after_mutation()
            .await;
        Ok(())
    }

    pub async fn remove(&self) -> Result<()> {
        if self.is_local() {
            let manifest = self
                .services
                .inner
                .local
                .runtime
                .get_service_manifest(&self.key.service_name);
            self.services
                .inner
                .local
                .runtime
                .remove_by_name(&self.key.service_name)
                .await?;
            sync_service_endpoint_listeners_for_manifest(
                &self.services.inner.local.tcp_tunneling,
                manifest.as_ref(),
                false,
            )
            .await?;
            return Ok(());
        }

        self.services
            .inner
            .remote
            .control
            .remove_peer_service(self.key.device_id, self.key.service_name.clone())
            .await?;
        self.device_services()
            .refresh_or_keep_after_mutation()
            .await;
        Ok(())
    }

    pub async fn logs(&self, tail: Option<usize>) -> Result<ServiceLogs> {
        if self.is_local() {
            return self
                .services
                .inner
                .local
                .runtime
                .logs_by_name(
                    &self.key.service_name,
                    &ServiceLogsOptions {
                        tail: tail.map(|tail| tail.to_string()),
                    },
                )
                .await;
        }

        let tail = tail
            .unwrap_or(crate::DEFAULT_REMOTE_SERVICE_LOG_TAIL)
            .clamp(1, crate::MAX_REMOTE_SERVICE_LOG_TAIL);
        let response = self
            .services
            .inner
            .remote
            .control
            .get_peer_service_logs(self.key.device_id, self.key.service_name.clone(), tail)
            .await?;
        let text = response
            .logs_text
            .ok_or_else(|| anyhow::anyhow!("remote service log response did not include logs"))?;
        Ok(ServiceLogs {
            raw: Vec::new(),
            text,
        })
    }

    pub(crate) fn forget_cached_observation(&self) -> Result<bool> {
        self.device_services()
            .remove_cached_service(&self.key.service_name)
    }
}

pub(crate) fn merge_device_service_snapshot(
    peer_id: PeerId,
    managed: Vec<ServiceInstance>,
    published: Vec<DeviceService>,
) -> DeviceServiceSnapshot {
    let mut services_by_name = BTreeMap::<String, DeviceService>::new();
    for service in managed {
        services_by_name.insert(service.name.clone(), device_service_from_instance(service));
    }
    for service in published {
        services_by_name
            .entry(service.name.clone())
            .and_modify(|existing| {
                existing.metadata = service.metadata.clone();
                existing.endpoints = service.endpoints.clone();
            })
            .or_insert(service);
    }

    DeviceServiceSnapshot {
        peer_id: peer_id.to_string(),
        services: services_by_name.into_values().collect(),
        updated_at: SystemTime::now(),
    }
}

fn device_service_from_instance(instance: ServiceInstance) -> DeviceService {
    DeviceService {
        name: instance.name,
        runtime: instance.runtime,
        metadata: Default::default(),
        endpoints: Vec::new(),
        status: instance.status,
    }
}

fn find_service(snapshot: DeviceServiceSnapshot, name: &str) -> Option<DeviceService> {
    snapshot
        .services
        .into_iter()
        .find(|service| service.name == name)
}

async fn with_remote_refresh_timeout<T>(refresh: impl Future<Output = Result<T>>) -> Result<T> {
    tokio::time::timeout(REMOTE_SERVICE_REFRESH_TIMEOUT, refresh)
        .await
        .unwrap_or_else(|_| {
            Err(anyhow::anyhow!(
                "remote service refresh timed out after {} seconds",
                REMOTE_SERVICE_REFRESH_TIMEOUT.as_secs()
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn remote_refresh_times_out_after_fifteen_seconds() {
        let result = with_remote_refresh_timeout(std::future::pending::<Result<()>>()).await;

        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("timed out after 15 seconds")
        );
    }

    #[test]
    fn snapshot_removal_rejects_only_writes_started_before_removal() {
        let fungi_dir = tempfile::tempdir().unwrap();
        let snapshots = DeviceServiceSnapshots::new(fungi_dir.path().to_path_buf());
        let peer_id = PeerId::random();
        let snapshot = DeviceServiceSnapshot {
            peer_id: peer_id.to_string(),
            services: Vec::new(),
            updated_at: SystemTime::now(),
        };
        let stale_epoch = snapshots.epoch(peer_id);

        assert!(
            snapshots
                .save_if_current(peer_id, stale_epoch, &snapshot)
                .unwrap()
        );
        assert!(snapshots.remove(peer_id).unwrap());

        assert!(
            !snapshots
                .save_if_current(peer_id, stale_epoch, &snapshot)
                .unwrap()
        );
        assert!(snapshots.get(peer_id).unwrap().is_none());

        let current_epoch = snapshots.epoch(peer_id);
        assert!(
            snapshots
                .save_if_current(peer_id, current_epoch, &snapshot)
                .unwrap()
        );
        assert!(snapshots.get(peer_id).unwrap().is_some());
    }
}
