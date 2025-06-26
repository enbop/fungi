use std::{net::IpAddr, path::PathBuf};

use anyhow::{Result, bail};
use fungi_config::file_transfer::{FileTransferClient, FtpProxy, WebdavProxy};
use libp2p::PeerId;

use crate::FungiDaemon;

impl FungiDaemon {
    pub fn host_name() -> Option<String> {
        fungi_util::sysinfo::System::host_name()
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

    pub fn get_incoming_allowed_peers_list(&self) -> Vec<String> {
        self.swarm_control()
            .state()
            .get_incoming_allowed_peers_list()
            .into_iter()
            .map(|p| p.to_string())
            .collect()
    }

    pub fn add_incoming_allowed_peer(&self, peer_id: PeerId) -> Result<()> {
        // update config and write config file
        self.config().lock().add_incoming_allowed_peer(&peer_id)?;
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
        self.config()
            .lock()
            .remove_incoming_allowed_peer(&peer_id)?;
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

    pub fn start_file_transfer_service(&self, root_dir: String) -> Result<()> {
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
        let config = self
            .config()
            .lock()
            .update_file_transfer_service(true, PathBuf::from(root_dir.clone()))?;

        self.fts_control().stop_all();
        self.fts_control()
            .add_service(config)
            .map_err(|e| anyhow::anyhow!("Failed to start file transfer service: {}", e))?;
        Ok(())
    }

    pub fn stop_file_transfer_service(&self) -> Result<()> {
        let current_root_dir = self.get_file_transfer_service_root_dir();
        if !self.fts_control().has_service(&current_root_dir) {
            bail!("File transfer service is not running.");
        }

        // update config and write config file
        self.config()
            .lock()
            .update_file_transfer_service(false, current_root_dir)?;

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
            if let Ok(remote_host_name) =
                ftc_control.connect_and_get_host_name(peer_id.clone()).await
            {
                name = remote_host_name
            }
        }

        self.config()
            .lock()
            .add_file_transfer_client(enabled, peer_id.clone(), name.clone())?;

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
        self.config().lock().remove_file_transfer_client(&peer_id)?;
        self.ftc_control().remove_client(&peer_id);
        Ok(())
    }

    pub async fn enable_file_transfer_client(&self, peer_id: PeerId, enabled: bool) -> Result<()> {
        let Some(mut config) = self
            .config()
            .lock()
            .enable_file_transfer_client(&peer_id, enabled)?
        else {
            bail!("File transfer client with peer_id {} not found", peer_id);
        };
        if enabled {
            if self.ftc_control().has_client(&peer_id) {
                return Ok(());
            }

            // TODO add transaction
            if config.name.is_none() {
                if let Ok(remote_host_name) = self
                    .ftc_control()
                    .connect_and_get_host_name(peer_id.clone())
                    .await
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
        self.config().lock().update_ftp_proxy(enabled, host, port)?;
        self.update_ftp_proxy_task(enabled, host, port)
    }

    pub fn get_webdav_proxy(&self) -> WebdavProxy {
        self.config().lock().file_transfer.proxy_webdav.clone()
    }

    pub fn update_webdav_proxy(&self, enabled: bool, host: IpAddr, port: u16) -> Result<()> {
        if port == 0 {
            bail!("Port must be greater than 0");
        }
        self.config()
            .lock()
            .update_webdav_proxy(enabled, host, port)?;
        self.update_webdav_proxy_task(enabled, host, port)
    }
}
