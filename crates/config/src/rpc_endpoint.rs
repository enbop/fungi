use anyhow::{Context, Result};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    net::SocketAddr,
    path::{Path, PathBuf},
};

pub const DAEMON_ENDPOINT_FILE: &str = "daemon.endpoint";

pub fn daemon_endpoint_path(fungi_dir: &Path) -> PathBuf {
    fungi_dir.join(DAEMON_ENDPOINT_FILE)
}

pub fn read_daemon_endpoint(fungi_dir: &Path) -> Result<String> {
    let path = daemon_endpoint_path(fungi_dir);
    let endpoint = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read daemon endpoint from {}", path.display()))?;
    let endpoint = endpoint.trim();
    if endpoint.is_empty() {
        anyhow::bail!("Daemon endpoint file is empty: {}", path.display());
    }
    Ok(endpoint.to_string())
}

pub struct PublishedDaemonEndpoint {
    path: PathBuf,
    endpoint: String,
}

impl PublishedDaemonEndpoint {
    pub fn publish(fungi_dir: &Path, address: SocketAddr) -> Result<Self> {
        fs::create_dir_all(fungi_dir).with_context(|| {
            format!("Failed to create Fungi directory: {}", fungi_dir.display())
        })?;

        let path = daemon_endpoint_path(fungi_dir);
        let temp_path = fungi_dir.join(format!(
            ".{DAEMON_ENDPOINT_FILE}.{}.tmp",
            std::process::id()
        ));
        let endpoint = format!("http://{address}");

        let result = (|| -> Result<()> {
            let mut options = OpenOptions::new();
            options.write(true).create(true).truncate(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&temp_path).with_context(|| {
                format!(
                    "Failed to create daemon endpoint at {}",
                    temp_path.display()
                )
            })?;
            writeln!(file, "{endpoint}").with_context(|| {
                format!("Failed to write daemon endpoint to {}", temp_path.display())
            })?;
            file.sync_all().with_context(|| {
                format!("Failed to sync daemon endpoint at {}", temp_path.display())
            })?;
            drop(file);

            fs::rename(&temp_path, &path).with_context(|| {
                format!(
                    "Failed to publish daemon endpoint from {} to {}",
                    temp_path.display(),
                    path.display()
                )
            })?;
            Ok(())
        })();

        if result.is_err() {
            let _ = fs::remove_file(&temp_path);
        }
        result?;

        Ok(Self { path, endpoint })
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
}

impl Drop for PublishedDaemonEndpoint {
    fn drop(&mut self) {
        let Ok(current) = fs::read_to_string(&self.path) else {
            return;
        };
        if current.trim() == self.endpoint {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    fn loopback(port: u16) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)
    }

    #[test]
    fn publishes_and_reads_daemon_endpoint() {
        let dir = tempfile::tempdir().unwrap();

        let publication = PublishedDaemonEndpoint::publish(dir.path(), loopback(61234)).unwrap();

        assert_eq!(
            read_daemon_endpoint(dir.path()).unwrap(),
            "http://127.0.0.1:61234"
        );
        assert_eq!(publication.endpoint(), "http://127.0.0.1:61234");
    }

    #[test]
    fn publication_only_removes_its_own_endpoint() {
        let dir = tempfile::tempdir().unwrap();
        let old = PublishedDaemonEndpoint::publish(dir.path(), loopback(61234)).unwrap();
        let current = PublishedDaemonEndpoint::publish(dir.path(), loopback(61235)).unwrap();

        drop(old);
        assert_eq!(
            read_daemon_endpoint(dir.path()).unwrap(),
            "http://127.0.0.1:61235"
        );

        drop(current);
        assert!(!daemon_endpoint_path(dir.path()).exists());
    }
}
