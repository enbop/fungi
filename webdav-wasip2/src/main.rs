use std::{env, net::SocketAddr};

use anyhow::{Context, bail};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::{Duration, timeout},
};
use webdav_wasip2::{MemoryBackend, WebDavFileSystem, serve, serve_listener};

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("--smoke-test") => run_smoke_test().await,
        Some("--addr") => {
            let addr: SocketAddr = args
                .next()
                .context("missing socket address after --addr")?
                .parse()
                .context("invalid socket address")?;
            run_server(addr).await
        }
        Some(other) => bail!("unknown argument: {other}"),
        None => run_server("127.0.0.1:8080".parse().unwrap()).await,
    }
}

async fn run_server(addr: SocketAddr) -> anyhow::Result<()> {
    let backend = MemoryBackend::demo();
    let filesystem = WebDavFileSystem::new(backend);
    serve(addr, filesystem).await
}

async fn run_smoke_test() -> anyhow::Result<()> {
    let backend = MemoryBackend::demo();
    let filesystem = WebDavFileSystem::new(backend);
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .context("failed to bind smoke-test listener")?;
    let addr = listener
        .local_addr()
        .context("failed to get smoke-test addr")?;

    let server = tokio::spawn(async move { serve_listener(listener, filesystem).await });
    let result = smoke_get(addr).await;
    server.abort();
    result
}

async fn smoke_get(addr: SocketAddr) -> anyhow::Result<()> {
    let mut stream = timeout(Duration::from_secs(5), TcpStream::connect(addr))
        .await
        .context("timed out connecting to webdav server")??;

    stream
        .write_all(b"GET /hello.txt HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await
        .context("failed to write request")?;
    stream.flush().await.context("failed to flush request")?;

    let mut response = Vec::new();
    timeout(Duration::from_secs(5), stream.read_to_end(&mut response))
        .await
        .context("timed out reading response")??;

    let response = String::from_utf8(response).context("response was not valid utf-8")?;
    if !response.starts_with("HTTP/1.1 200") {
        bail!("unexpected response status: {response}");
    }
    if !response.contains("hello from webdav-wasip2") {
        bail!("unexpected response body: {response}");
    }

    println!("smoke test passed against http://{addr}/hello.txt");
    Ok(())
}
