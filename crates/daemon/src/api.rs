use std::{net::IpAddr, path::PathBuf};

use anyhow::{Result, bail};
use fungi_config::address_book::PeerInfo;
use fungi_config::file_transfer::{FileTransferClient, FtpProxy, WebdavProxy};
use libp2p::PeerId;

use crate::FungiDaemon;

impl FungiDaemon {
    pub fn host_name(&self) -> Option<String> {
        self.config().lock().get_hostname()
    }

    #[cfg(target_os = "android")]
    pub fn init_mobile_device_name(name: String) {
        {
            fungi_util::init_mobile_device_name(name);
        }
    }

    pub fn peer_id(&self) -> String {
        self.swarm_control().local_peer_id().to_string()
    }

    pub fn config_file_path(&self) -> String {
        self.config()
            .lock()
            .config_file_path()
            .to_string_lossy()
            .to_string()
    }

    pub fn add_incoming_allowed_peer(&self, peer_id: PeerId) -> Result<()> {
        // update config and write config file
        let current_config = self.config().lock().clone();
        let updated_config = current_config.add_incoming_allowed_peer(&peer_id)?;
        *self.config().lock() = updated_config;

        // update state
        self.swarm_control()
            .state()
            .incoming_allowed_peers()
            .write()
            .insert(peer_id);
        Ok(())
    }

    pub fn remove_incoming_allowed_peer(&self, peer_id: PeerId) -> Result<()> {
        // update config and write config file
        let current_config = self.config().lock().clone();
        let updated_config = current_config.remove_incoming_allowed_peer(&peer_id)?;
        *self.config().lock() = updated_config;
        // update state
        self.swarm_control()
            .state()
            .incoming_allowed_peers()
            .write()
            .remove(&peer_id);
        // TODO disconnect connected incoming peer
        Ok(())
    }

    pub fn get_file_transfer_service_enabled(&self) -> bool {
        self.config().lock().file_transfer.server.enabled
    }

    pub fn get_file_transfer_service_root_dir(&self) -> PathBuf {
        self.config()
            .lock()
            .file_transfer
            .server
            .shared_root_dir
            .clone()
    }

    pub async fn start_file_transfer_service(&self, root_dir: String) -> Result<()> {
        if self.get_file_transfer_service_root_dir().to_str() == Some(&root_dir)
            && self
                .fts_control()
                .has_service(&PathBuf::from(root_dir.clone()))
        {
            bail!(
                "File transfer service is already running with the root directory: {}",
                root_dir
            );
        }

        // update config and write config file
        let current_config = self.config().lock().clone();
        let (updated_config, service_config) =
            current_config.update_file_transfer_service(true, PathBuf::from(root_dir.clone()))?;
        *self.config().lock() = updated_config;

        self.fts_control().stop_all();
        // FIXME: This is a workaround to ensure the service is stopped before starting it again.
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        self.fts_control()
            .clone()
            .add_service(service_config)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to start file transfer service: {}", e))?;
        Ok(())
    }

    pub fn stop_file_transfer_service(&self) -> Result<()> {
        let current_root_dir = self.get_file_transfer_service_root_dir();
        if !self.fts_control().has_service(&current_root_dir) {
            bail!("File transfer service is not running.");
        }

        // update config and write config file
        let current_config = self.config().lock().clone();
        let (updated_config, _) =
            current_config.update_file_transfer_service(false, current_root_dir)?;
        *self.config().lock() = updated_config;

        self.fts_control().stop_all();
        Ok(())
    }

    pub async fn add_file_transfer_client(
        &self,
        enabled: bool,
        mut name: Option<String>,
        peer_id: PeerId,
    ) -> Result<()> {
        let ftc_control = self.ftc_control();

        // TODO add transaction
        if name.is_none() {
            if let Ok(remote_host_name) = ftc_control.connect_and_get_host_name(peer_id).await {
                name = remote_host_name
            }
        }

        let current_config = self.config().lock().clone();
        let updated_config =
            current_config.add_file_transfer_client(enabled, peer_id, name.clone())?;
        *self.config().lock() = updated_config;

        if enabled {
            ftc_control.add_client(FileTransferClient {
                enabled,
                peer_id,
                name,
            });
        }

        Ok(())
    }

    pub fn remove_file_transfer_client(&self, peer_id: PeerId) -> Result<()> {
        let current_config = self.config().lock().clone();
        let updated_config = current_config.remove_file_transfer_client(&peer_id)?;
        *self.config().lock() = updated_config;
        self.ftc_control().remove_client(&peer_id);
        Ok(())
    }

    pub async fn enable_file_transfer_client(&self, peer_id: PeerId, enabled: bool) -> Result<()> {
        let current_config = self.config().lock().clone();
        let (updated_config, client_option) =
            current_config.enable_file_transfer_client(&peer_id, enabled)?;
        *self.config().lock() = updated_config;

        let Some(mut config) = client_option else {
            bail!("File transfer client with peer_id {} not found", peer_id);
        };
        if enabled {
            if self.ftc_control().has_client(&peer_id) {
                return Ok(());
            }

            // TODO add transaction
            if config.name.is_none() {
                if let Ok(remote_host_name) =
                    self.ftc_control().connect_and_get_host_name(peer_id).await
                {
                    config.name = remote_host_name
                }
            }
            self.ftc_control().add_client(config);
        } else {
            self.ftc_control().remove_client(&peer_id);
        }

        Ok(())
    }

    pub fn get_all_file_transfer_clients(&self) -> Vec<FileTransferClient> {
        self.config().lock().file_transfer.client.clone()
    }

    pub fn get_ftp_proxy(&self) -> FtpProxy {
        self.config().lock().file_transfer.proxy_ftp.clone()
    }

    pub fn update_ftp_proxy(&self, enabled: bool, host: IpAddr, port: u16) -> Result<()> {
        if port == 0 {
            bail!("Port must be greater than 0");
        }
        let current_config = self.config().lock().clone();
        let updated_config = current_config.update_ftp_proxy(enabled, host, port)?;
        *self.config().lock() = updated_config;
        self.update_ftp_proxy_task(enabled, host, port)
    }

    pub fn get_webdav_proxy(&self) -> WebdavProxy {
        self.config().lock().file_transfer.proxy_webdav.clone()
    }

    pub fn update_webdav_proxy(&self, enabled: bool, host: IpAddr, port: u16) -> Result<()> {
        if port == 0 {
            bail!("Port must be greater than 0");
        }
        let current_config = self.config().lock().clone();
        let updated_config = current_config.update_webdav_proxy(enabled, host, port)?;
        *self.config().lock() = updated_config;
        self.update_webdav_proxy_task(enabled, host, port)
    }

    // TCP Tunneling API methods
    pub fn get_tcp_forwarding_rules(
        &self,
    ) -> Vec<(String, fungi_config::tcp_tunneling::ForwardingRule)> {
        self.tcp_tunneling_control().get_forwarding_rules()
    }

    pub fn get_tcp_listening_rules(
        &self,
    ) -> Vec<(String, fungi_config::tcp_tunneling::ListeningRule)> {
        self.tcp_tunneling_control().get_listening_rules()
    }

    pub async fn add_tcp_forwarding_rule(
        &self,
        local_host: String,
        local_port: u16,
        remote_peer_id: String,
        remote_port: u16,
    ) -> Result<String> {
        let rule = fungi_config::tcp_tunneling::ForwardingRule {
            local_host,
            local_port,
            remote_peer_id,
            remote_port,
        };
        self.add_tcp_forwarding_rule_internal(rule).await
    }

    pub fn remove_tcp_forwarding_rule(&self, rule_id: String) -> Result<()> {
        self.remove_tcp_forwarding_rule_internal(&rule_id)
    }

    pub async fn add_tcp_listening_rule(
        &self,
        local_host: String,
        local_port: u16,
        _allowed_peers: Vec<String>,
    ) -> Result<String> {
        let rule = fungi_config::tcp_tunneling::ListeningRule {
            host: local_host,
            port: local_port,
        };
        self.add_tcp_listening_rule_internal(rule).await
    }

    pub fn remove_tcp_listening_rule(&self, rule_id: String) -> Result<()> {
        self.remove_tcp_listening_rule_internal(&rule_id)
    }

    pub fn get_tcp_tunneling_config(&self) -> fungi_config::tcp_tunneling::TcpTunneling {
        self.config().lock().tcp_tunneling.clone()
    }

    pub async fn mdns_get_local_devices(&self) -> Result<Vec<PeerInfo>> {
        let local_devices = self
            .mdns_control()
            .get_all_devices()
            .values()
            .into_iter()
            .cloned()
            .collect();
        Ok(local_devices)
    }

    pub fn address_book_get_all(&self) -> Vec<PeerInfo> {
        self.address_book().lock().get_all_peers().clone()
    }

    pub fn address_book_add_or_update(&self, peer_info: PeerInfo) -> Result<()> {
        let current_peers_config = self.address_book().lock().clone();
        let updated_peers_config = current_peers_config.add_or_update_peer(peer_info)?;
        *self.address_book().lock() = updated_peers_config;
        Ok(())
    }

    pub fn address_book_get_peer(&self, peer_id: PeerId) -> Option<PeerInfo> {
        self.address_book().lock().get_peer_info(&peer_id).cloned()
    }

    pub fn address_book_remove(&self, peer_id: PeerId) -> Result<()> {
        let current_peers_config = self.address_book().lock().clone();
        let updated_peers_config = current_peers_config.remove_peer(&peer_id)?;
        *self.address_book().lock() = updated_peers_config;
        Ok(())
    }

    pub fn get_incoming_allowed_peers(&self) -> Vec<PeerInfo> {
        let allowed_peers = self
            .swarm_control()
            .state()
            .get_incoming_allowed_peers_list();
        let peers_config_guard = self.address_book();
        let peers_config = peers_config_guard.lock();

        allowed_peers
            .into_iter()
            .map(
                |peer_id| match peers_config.get_peer_info(&peer_id).cloned() {
                    Some(peer_info) => peer_info,
                    None => PeerInfo::new_unknown(peer_id),
                },
            )
            .collect()
    }
}
