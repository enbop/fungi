use std::{convert::Infallible, net::SocketAddr};

use anyhow::Context;
use dav_server::{DavHandler, fakels::FakeLs};
use hyper::{server::conn::http1, service::service_fn};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;

use crate::{WebDavBackend, WebDavFileSystem};

pub async fn serve<B>(addr: SocketAddr, filesystem: WebDavFileSystem<B>) -> anyhow::Result<()>
where
    B: WebDavBackend,
{
    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind {addr}"))?;
    serve_listener(listener, filesystem).await
}

pub async fn serve_listener<B>(
    listener: TcpListener,
    filesystem: WebDavFileSystem<B>,
) -> anyhow::Result<()>
where
    B: WebDavBackend,
{
    let local_addr = listener.local_addr().context("failed to read local addr")?;
    let dav_handler = DavHandler::builder()
        .filesystem(Box::new(filesystem))
        .locksystem(FakeLs::new())
        .build_handler();

    log::info!("listening on http://{local_addr}");

    loop {
        let (stream, _) = listener.accept().await.context("accept failed")?;
        let dav_handler = dav_handler.clone();
        tokio::spawn(async move {
            let io = TokioIo::new(stream);
            if let Err(error) = http1::Builder::new()
                .serve_connection(
                    io,
                    service_fn(move |request| {
                        let dav_handler = dav_handler.clone();
                        async move { Ok::<_, Infallible>(dav_handler.handle(request).await) }
                    }),
                )
                .await
            {
                log::error!("connection failed: {error:?}");
            }
        });
    }
}
