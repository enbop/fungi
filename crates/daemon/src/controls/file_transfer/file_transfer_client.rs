use std::{
    collections::HashMap,
    convert::Infallible,
    sync::{Arc, Mutex},
    time::Duration,
};

use dav_server::DavHandler;
use fungi_config::file_transfer::FileTransferClient as FileTransferClientConfig;
use fungi_swarm::SwarmControl;
use fungi_util::protocols::FUNGI_FILE_TRANSFER_PROTOCOL;
use hyper::{server::conn::http1, service::service_fn};
use hyper_util::rt::TokioIo;
use libp2p::{PeerId, Stream};
use tarpc::{serde_transport, tokio_serde::formats::Bincode};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_util::{codec::LengthDelimitedCodec, compat::FuturesAsyncReadCompatExt as _};

use crate::controls::file_transfer::FileTransferRpcClient;

#[derive(Clone)]
pub struct FileTransferClientControl {
    swarm_control: SwarmControl,
    clients: Arc<Mutex<HashMap<PeerId, JoinHandle<()>>>>,
}

impl FileTransferClientControl {
    pub fn new(swarm_control: SwarmControl) -> Self {
        Self {
            swarm_control,
            clients: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn start_client(&self, config: FileTransferClientConfig) {
        let client_handle = tokio::spawn(start_file_transfer_client(
            config.clone(),
            self.swarm_control.clone(),
        ));
        self.clients
            .lock()
            .unwrap()
            .insert(config.target_peer, client_handle);
    }
}

fn connect_file_transfer_rpc(stream: Stream) -> FileTransferRpcClient {
    let codec_builder = LengthDelimitedCodec::builder();
    let transport = serde_transport::new(
        codec_builder.new_framed(stream.compat()),
        Bincode::default(),
    );
    FileTransferRpcClient::new(Default::default(), transport).spawn()
}

async fn start_file_transfer_client(
    config: FileTransferClientConfig,
    mut swarm_control: SwarmControl,
) {
    loop {
        if let Err(e) = swarm_control
            .invoke_swarm(move |swarm| swarm.dial(config.target_peer))
            .await
            .unwrap()
        {
            log::error!(
                "Failed to dial peer {}: {}. Retrying in 5 seconds...",
                config.target_peer,
                e
            );
            tokio::time::sleep(Duration::from_secs(5)).await;
            continue;
        };

        let Ok(stream) = swarm_control
            .stream_control
            .open_stream(config.target_peer, FUNGI_FILE_TRANSFER_PROTOCOL)
            .await
        else {
            log::error!("Failed to open stream to peer {}", config.target_peer);
            tokio::time::sleep(Duration::from_secs(5)).await;
            continue;
        };
        let client = connect_file_transfer_rpc(stream);

        tokio::spawn(start_webdav_proxy_service(
            "0.0.0.0".into(),
            9005,
            client.clone(),
        ));
        start_ftp_proxy_service(config.proxy_ftp_host.clone(), config.proxy_ftp_port, client).await;
    }
}

async fn start_ftp_proxy_service(host: String, port: u16, client: FileTransferRpcClient) {
    loop {
        let client_cl = client.clone();
        let server = libunftp::ServerBuilder::new(Box::new(move || client_cl.clone()))
            .greeting("Welcome to Fungi FTP proxy")
            .passive_ports(50000..=65535)
            .build()
            .unwrap();

        log::info!("Starting FTP proxy service on port {}", port);
        if let Err(e) = server.listen(format!("{}:{}", host, port)).await {
            log::error!(
                "Failed to start FTP proxy service on port {}: {}. Retrying in 5 seconds...",
                port,
                e
            );
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

async fn start_webdav_proxy_service(
    host: String,
    port: u16,
    client: FileTransferRpcClient,
) -> JoinHandle<()> {
    let dav_server = DavHandler::builder()
        .filesystem(Box::new(client))
        .build_handler();

    let addr = format!("{}:{}", host, port);
    println!("Listening webdav on {addr}");
    let listener = TcpListener::bind(addr).await.unwrap();

    // We start a loop to continuously accept incoming connections
    loop {
        let (stream, _) = listener.accept().await.unwrap();
        let dav_server = dav_server.clone();

        // Use an adapter to access something implementing `tokio::io` traits as if they implement
        // `hyper::rt` IO traits.
        let io = TokioIo::new(stream);

        // Spawn a tokio task to serve multiple connections concurrently
        tokio::task::spawn(async move {
            // Finally, we bind the incoming connection to our `hello` service
            if let Err(err) = http1::Builder::new()
                // `service_fn` converts our function in a `Service`
                .serve_connection(
                    io,
                    service_fn({
                        move |req| {
                            let dav_server = dav_server.clone();
                            async move { Ok::<_, Infallible>(dav_server.handle(req).await) }
                        }
                    }),
                )
                .await
            {
                eprintln!("Failed serving: {err:?}");
            }
        });
    }
}
