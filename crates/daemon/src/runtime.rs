use std::{collections::BTreeMap, path::PathBuf};

use anyhow::{Result, bail};
use async_trait::async_trait;
use fungi_docker_agent::{ContainerSpec, LogsOptions, PortProtocol};

use crate::controls::DockerControl;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeKind {
    Docker,
    Wasmtime,
}

#[derive(Debug, Clone)]
pub struct ServiceManifest {
    pub name: String,
    pub runtime: RuntimeKind,
    pub source: ServiceSource,
    pub env: BTreeMap<String, String>,
    pub mounts: Vec<ServiceMount>,
    pub ports: Vec<ServicePort>,
    pub command: Vec<String>,
    pub entrypoint: Vec<String>,
    pub working_dir: Option<String>,
    pub labels: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub enum ServiceSource {
    Docker { image: String },
    Wasmtime { component: PathBuf },
}

#[derive(Debug, Clone)]
pub struct ServiceMount {
    pub host_path: PathBuf,
    pub runtime_path: String,
}

#[derive(Debug, Clone)]
pub struct ServicePort {
    pub host_port: u16,
    pub service_port: u16,
    pub protocol: ServicePortProtocol,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServicePortProtocol {
    Tcp,
    Udp,
}

#[derive(Debug, Clone)]
pub struct ServiceInstance {
    pub runtime: RuntimeKind,
    pub handle: String,
    pub name: String,
    pub source: String,
    pub labels: BTreeMap<String, String>,
    pub status: ServiceStatus,
}

#[derive(Debug, Clone)]
pub struct ServiceStatus {
    pub state: String,
    pub running: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ServiceLogsOptions {
    pub tail: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ServiceLogs {
    pub raw: Vec<u8>,
    pub text: String,
}

#[async_trait]
pub trait RuntimeProvider: Send + Sync {
    fn runtime_kind(&self) -> RuntimeKind;
    async fn deploy(&self, manifest: &ServiceManifest) -> Result<ServiceInstance>;
    async fn start(&self, handle: &str) -> Result<()>;
    async fn stop(&self, handle: &str) -> Result<()>;
    async fn remove(&self, handle: &str) -> Result<()>;
    async fn inspect(&self, handle: &str) -> Result<ServiceInstance>;
    async fn logs(&self, handle: &str, options: &ServiceLogsOptions) -> Result<ServiceLogs>;
}

#[derive(Clone)]
pub struct DockerRuntimeProvider {
    docker: DockerControl,
}

impl DockerRuntimeProvider {
    pub fn new(docker: DockerControl) -> Self {
        Self { docker }
    }
}

#[async_trait]
impl RuntimeProvider for DockerRuntimeProvider {
    fn runtime_kind(&self) -> RuntimeKind {
        RuntimeKind::Docker
    }

    async fn deploy(&self, manifest: &ServiceManifest) -> Result<ServiceInstance> {
        let spec = docker_spec_from_manifest(manifest)?;
        let details = self.docker.create_container(&spec).await?;
        Ok(map_docker_instance(details))
    }

    async fn start(&self, handle: &str) -> Result<()> {
        self.docker.start_container(handle).await
    }

    async fn stop(&self, handle: &str) -> Result<()> {
        self.docker.stop_container(handle).await
    }

    async fn remove(&self, handle: &str) -> Result<()> {
        self.docker.remove_container(handle).await
    }

    async fn inspect(&self, handle: &str) -> Result<ServiceInstance> {
        let details = self.docker.inspect_container(handle).await?;
        Ok(map_docker_instance(details))
    }

    async fn logs(&self, handle: &str, options: &ServiceLogsOptions) -> Result<ServiceLogs> {
        let logs = self
            .docker
            .container_logs(
                handle,
                &LogsOptions {
                    stdout: true,
                    stderr: true,
                    tail: options.tail.clone(),
                },
            )
            .await?;
        Ok(ServiceLogs {
            raw: logs.raw,
            text: logs.text,
        })
    }
}

#[derive(Clone, Default)]
pub struct WasmtimeRuntimeProvider;

#[async_trait]
impl RuntimeProvider for WasmtimeRuntimeProvider {
    fn runtime_kind(&self) -> RuntimeKind {
        RuntimeKind::Wasmtime
    }

    async fn deploy(&self, manifest: &ServiceManifest) -> Result<ServiceInstance> {
        ensure_wasmtime_manifest(manifest)?;
        bail!("wasmtime runtime provider is not implemented yet")
    }

    async fn start(&self, _handle: &str) -> Result<()> {
        bail!("wasmtime runtime provider is not implemented yet")
    }

    async fn stop(&self, _handle: &str) -> Result<()> {
        bail!("wasmtime runtime provider is not implemented yet")
    }

    async fn remove(&self, _handle: &str) -> Result<()> {
        bail!("wasmtime runtime provider is not implemented yet")
    }

    async fn inspect(&self, _handle: &str) -> Result<ServiceInstance> {
        bail!("wasmtime runtime provider is not implemented yet")
    }

    async fn logs(&self, _handle: &str, _options: &ServiceLogsOptions) -> Result<ServiceLogs> {
        bail!("wasmtime runtime provider is not implemented yet")
    }
}

#[derive(Clone, Default)]
pub struct RuntimeControl {
    docker: Option<DockerRuntimeProvider>,
    wasmtime: WasmtimeRuntimeProvider,
}

impl RuntimeControl {
    pub fn new(docker: Option<DockerControl>) -> Self {
        Self {
            docker: docker.map(DockerRuntimeProvider::new),
            wasmtime: WasmtimeRuntimeProvider,
        }
    }

    pub fn supports(&self, runtime: RuntimeKind) -> bool {
        match runtime {
            RuntimeKind::Docker => self.docker.is_some(),
            RuntimeKind::Wasmtime => true,
        }
    }

    pub async fn deploy(&self, manifest: &ServiceManifest) -> Result<ServiceInstance> {
        match manifest.runtime {
            RuntimeKind::Docker => self.docker_provider()?.deploy(manifest).await,
            RuntimeKind::Wasmtime => self.wasmtime.deploy(manifest).await,
        }
    }

    pub async fn start(&self, runtime: RuntimeKind, handle: &str) -> Result<()> {
        match runtime {
            RuntimeKind::Docker => self.docker_provider()?.start(handle).await,
            RuntimeKind::Wasmtime => self.wasmtime.start(handle).await,
        }
    }

    pub async fn stop(&self, runtime: RuntimeKind, handle: &str) -> Result<()> {
        match runtime {
            RuntimeKind::Docker => self.docker_provider()?.stop(handle).await,
            RuntimeKind::Wasmtime => self.wasmtime.stop(handle).await,
        }
    }

    pub async fn remove(&self, runtime: RuntimeKind, handle: &str) -> Result<()> {
        match runtime {
            RuntimeKind::Docker => self.docker_provider()?.remove(handle).await,
            RuntimeKind::Wasmtime => self.wasmtime.remove(handle).await,
        }
    }

    pub async fn inspect(&self, runtime: RuntimeKind, handle: &str) -> Result<ServiceInstance> {
        match runtime {
            RuntimeKind::Docker => self.docker_provider()?.inspect(handle).await,
            RuntimeKind::Wasmtime => self.wasmtime.inspect(handle).await,
        }
    }

    pub async fn logs(
        &self,
        runtime: RuntimeKind,
        handle: &str,
        options: &ServiceLogsOptions,
    ) -> Result<ServiceLogs> {
        match runtime {
            RuntimeKind::Docker => self.docker_provider()?.logs(handle, options).await,
            RuntimeKind::Wasmtime => self.wasmtime.logs(handle, options).await,
        }
    }

    fn docker_provider(&self) -> Result<&DockerRuntimeProvider> {
        self.docker
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("docker runtime is not enabled in config"))
    }
}

fn docker_spec_from_manifest(manifest: &ServiceManifest) -> Result<ContainerSpec> {
    if manifest.runtime != RuntimeKind::Docker {
        bail!("service manifest runtime does not match docker provider")
    }

    let ServiceSource::Docker { image } = &manifest.source else {
        bail!("docker runtime requires a docker image source")
    };

    Ok(ContainerSpec {
        name: Some(manifest.name.clone()),
        image: image.clone(),
        env: manifest.env.clone(),
        mounts: manifest
            .mounts
            .iter()
            .map(|mount| fungi_docker_agent::BindMount {
                host_path: mount.host_path.clone(),
                container_path: mount.runtime_path.clone(),
            })
            .collect(),
        ports: manifest
            .ports
            .iter()
            .map(|port| fungi_docker_agent::PortBinding {
                host_port: port.host_port,
                container_port: port.service_port,
                protocol: match port.protocol {
                    ServicePortProtocol::Tcp => PortProtocol::Tcp,
                    ServicePortProtocol::Udp => PortProtocol::Udp,
                },
            })
            .collect(),
        command: manifest.command.clone(),
        entrypoint: manifest.entrypoint.clone(),
        working_dir: manifest.working_dir.clone(),
        labels: manifest.labels.clone(),
    })
}

fn ensure_wasmtime_manifest(manifest: &ServiceManifest) -> Result<()> {
    if manifest.runtime != RuntimeKind::Wasmtime {
        bail!("service manifest runtime does not match wasmtime provider")
    }

    match &manifest.source {
        ServiceSource::Wasmtime { component } => {
            if component.as_os_str().is_empty() {
                bail!("wasmtime component path must not be empty");
            }
            Ok(())
        }
        ServiceSource::Docker { .. } => bail!("wasmtime runtime requires a wasm component source"),
    }
}

fn map_docker_instance(details: fungi_docker_agent::ContainerDetails) -> ServiceInstance {
    ServiceInstance {
        runtime: RuntimeKind::Docker,
        handle: details.id.clone(),
        name: details.name,
        source: details.image,
        labels: details.labels,
        status: ServiceStatus {
            state: details.state.status,
            running: details.state.running,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docker_manifest_maps_to_container_spec() {
        let manifest = ServiceManifest {
            name: "filebrowser".into(),
            runtime: RuntimeKind::Docker,
            source: ServiceSource::Docker {
                image: "filebrowser/filebrowser:latest".into(),
            },
            env: BTreeMap::from([(String::from("FB_NOAUTH"), String::from("true"))]),
            mounts: vec![ServiceMount {
                host_path: PathBuf::from("/tmp/fungi/data"),
                runtime_path: "/srv".into(),
            }],
            ports: vec![ServicePort {
                host_port: 8080,
                service_port: 80,
                protocol: ServicePortProtocol::Tcp,
            }],
            command: vec!["serve".into()],
            entrypoint: Vec::new(),
            working_dir: None,
            labels: BTreeMap::new(),
        };

        let spec = docker_spec_from_manifest(&manifest).unwrap();
        assert_eq!(spec.name.as_deref(), Some("filebrowser"));
        assert_eq!(spec.image, "filebrowser/filebrowser:latest");
        assert_eq!(spec.ports[0].host_port, 8080);
    }

    #[test]
    fn docker_manifest_rejects_wrong_source_type() {
        let manifest = ServiceManifest {
            name: "bad".into(),
            runtime: RuntimeKind::Docker,
            source: ServiceSource::Wasmtime {
                component: PathBuf::from("/tmp/app.wasm"),
            },
            env: BTreeMap::new(),
            mounts: Vec::new(),
            ports: Vec::new(),
            command: Vec::new(),
            entrypoint: Vec::new(),
            working_dir: None,
            labels: BTreeMap::new(),
        };

        assert!(docker_spec_from_manifest(&manifest).is_err());
    }
}