use rustls_pemfile::{certs, rsa_private_keys};
use tokio_rustls::server::TlsStream;
use std::fs::File;
use std::io::{self, BufReader};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::rustls::{self, Certificate, PrivateKey};
use tokio_rustls::TlsAcceptor;

fn load_certs(path: &Path) -> io::Result<Vec<Certificate>> {
    certs(&mut BufReader::new(File::open(path)?))
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid cert"))
        .map(|mut certs| certs.drain(..).map(Certificate).collect())
}

fn load_keys(path: &Path) -> io::Result<Vec<PrivateKey>> {
    rsa_private_keys(&mut BufReader::new(File::open(path)?))
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid key"))
        .map(|mut keys| keys.drain(..).map(PrivateKey).collect())
}

pub async fn start_tcp_tls_listener(
    addr: &str,
    cert_path: &PathBuf,
    key_path: &PathBuf,
    on_stream: impl Fn(TlsStream<TcpStream>, SocketAddr) + Send + Copy + Sync + 'static,
) -> io::Result<()> {
    let certs = load_certs(cert_path)?;
    let mut keys = load_keys(key_path)?;

    let config = rustls::ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth()
        .with_single_cert(certs, keys.remove(0))
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;

    let acceptor = TlsAcceptor::from(Arc::new(config));

    let listener = TcpListener::bind(addr).await?;
    loop {
        let (socket, addr) = listener.accept().await?;
        let acceptor = acceptor.clone();
        tokio::spawn(async move {
            if let Ok(socket) = acceptor.accept(socket).await {
                on_stream(socket, addr);
            }
        });
    }
}