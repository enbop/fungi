use clap::Parser;
use fungi_daemon::{
    RuntimeControl, RuntimeKind, ServiceLogsOptions, ServiceManifest, ServiceMount, ServicePort,
    ServicePortProtocol, ServiceRunMode, ServiceSource,
};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    net::TcpStream,
    path::PathBuf,
    time::Duration,
};

#[derive(Debug, Parser)]
struct Args {
    #[arg(long)]
    launcher: PathBuf,
    #[arg(long)]
    runtime_root: Option<PathBuf>,
    #[arg(long)]
    wasm_path: Option<PathBuf>,
    #[arg(long)]
    wasm_url: Option<String>,
    #[arg(long, default_value = "fungi-wasi-smoke")]
    name: String,
    #[arg(long, default_value_t = 18081)]
    port: u16,
    #[arg(long)]
    mount_dir: PathBuf,
    #[arg(long, default_value = "data")]
    mount_target: String,
    #[arg(long, default_value_t = 3)]
    wait_secs: u64,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let runtime_root = args.runtime_root.clone().unwrap_or_else(|| {
        args.mount_dir
            .parent()
            .unwrap_or(&args.mount_dir)
            .join("runtime")
    });

    fs::create_dir_all(&args.mount_dir)?;
    let provider = fungi_daemon::runtime::WasmtimeRuntimeProvider::new(
        runtime_root,
        args.launcher,
        args.mount_dir.clone(),
        vec![args.mount_dir.clone()],
    );
    let runtime = RuntimeControl::with_wasmtime_provider(
        provider,
        None,
        args.mount_dir.join("services"),
        true,
    )?;

    let source = if let Some(path) = args.wasm_path.clone() {
        ServiceSource::WasmtimeFile { component: path }
    } else if let Some(url) = args.wasm_url.clone() {
        ServiceSource::WasmtimeUrl { url }
    } else {
        return Err("either --wasm-path or --wasm-url is required".into());
    };

    let manifest = ServiceManifest {
        name: args.name.clone(),
        definition_id: None,
        runtime: RuntimeKind::Wasmtime,
        run_mode: ServiceRunMode::Command,
        source,
        expose: None,
        env: BTreeMap::new(),
        mounts: vec![ServiceMount {
            host_path: args.mount_dir.clone(),
            runtime_path: args.mount_target.clone(),
        }],
        ports: vec![ServicePort {
            name: None,
            host_port: args.port,
            host_port_allocation: fungi_daemon::ServicePortAllocation::Fixed,
            service_port: args.port,
            protocol: ServicePortProtocol::Tcp,
        }],
        command: Vec::new(),
        entrypoint: Vec::new(),
        working_dir: None,
        labels: BTreeMap::new(),
    };

    let _ = runtime.remove(RuntimeKind::Wasmtime, &args.name).await;

    let pulled = runtime.pull(&manifest).await?;
    println!("pull:\n{:#?}", pulled);

    let inspected = runtime.inspect(RuntimeKind::Wasmtime, &args.name).await?;
    println!("inspect.before_start:\n{:#?}", inspected);

    runtime.start(RuntimeKind::Wasmtime, &args.name).await?;
    println!("start: ok");

    tokio::time::sleep(Duration::from_secs(args.wait_secs)).await;
    let response = http_get(args.port)?;
    println!("http.get:\n{response}");

    let logs = runtime
        .logs(
            RuntimeKind::Wasmtime,
            &args.name,
            &ServiceLogsOptions {
                tail: Some("50".into()),
            },
        )
        .await?;
    println!("logs.text:\n{}", logs.text);

    runtime.stop(RuntimeKind::Wasmtime, &args.name).await?;
    println!("stop: ok");

    let inspected = runtime.inspect(RuntimeKind::Wasmtime, &args.name).await?;
    println!("inspect.after_stop:\n{:#?}", inspected);

    runtime.remove(RuntimeKind::Wasmtime, &args.name).await?;
    println!("remove: ok");
    Ok(())
}

fn http_get(port: u16) -> Result<String, Box<dyn std::error::Error>> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.write_all(
        b"GET /?device=smoke HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    )?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    Ok(response)
}
