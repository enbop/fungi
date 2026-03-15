use clap::{Parser, Subcommand};
use fungi_docker_agent::{
    AgentPolicy, BindMount, ContainerSpec, DockerAgent, LogsOptions, PortBinding, PortRule,
};
use std::{
    collections::BTreeMap,
    env, fs,
    io::{Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Debug, Parser)]
struct Args {
    #[arg(long)]
    socket: Option<PathBuf>,
    #[arg(long, default_value = "managed_by")]
    label_key: String,
    #[arg(long, default_value = "fungi")]
    label_value: String,
    #[arg(long)]
    allowed_root: PathBuf,
    #[arg(long)]
    mount_host: PathBuf,
    #[arg(long, default_value = "/usr/share/nginx/html")]
    mount_target: String,
    #[arg(long, default_value = "fungi-smoke-nginx")]
    name: String,
    #[arg(long, default_value = "nginx:alpine")]
    image: String,
    #[arg(long, default_value_t = 18080)]
    host_port: u16,
    #[arg(long, default_value_t = 80)]
    container_port: u16,
    #[arg(long, default_value_t = 1)]
    wait_secs: u64,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Create,
    Inspect,
    Start,
    Logs,
    Stop,
    Remove,
    RunAll,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let socket_path = resolve_socket_path(args.socket.as_deref())?;
    let agent = DockerAgent::new(AgentPolicy {
        socket_path,
        managed_label_key: args.label_key.clone(),
        managed_label_value: args.label_value.clone(),
        allowed_host_paths: vec![args.allowed_root.clone()],
        allowed_ports: vec![PortRule::Single(args.host_port)],
    });

    match args.command {
        Command::Create => {
            prepare_mount_dir(&args.mount_host)?;
            let details = agent.create_container(&build_spec(&args)).await?;
            print_json("create", &details)?;
        }
        Command::Inspect => {
            let details = agent.inspect_container(&args.name).await?;
            print_json("inspect", &details)?;
        }
        Command::Start => {
            agent.start_container(&args.name).await?;
            println!("start: ok");
        }
        Command::Logs => {
            let logs = agent
                .container_logs(&args.name, &LogsOptions::default())
                .await?;
            println!("logs.text:\n{}", logs.text);
        }
        Command::Stop => {
            agent.stop_container(&args.name).await?;
            println!("stop: ok");
        }
        Command::Remove => {
            agent.remove_container(&args.name).await?;
            println!("remove: ok");
        }
        Command::RunAll => {
            run_all(&agent, &args).await?;
        }
    }

    Ok(())
}

async fn run_all(agent: &DockerAgent, args: &Args) -> Result<(), Box<dyn std::error::Error>> {
    prepare_mount_dir(&args.mount_host)?;

    match agent.remove_container(&args.name).await {
        Ok(()) => println!("cleanup.remove_existing: ok"),
        Err(error) => println!("cleanup.remove_existing: skip ({error})"),
    }

    let spec = build_spec(args);
    let created = agent.create_container(&spec).await?;
    print_json("create", &created)?;

    let inspected = agent.inspect_container(&args.name).await?;
    print_json("inspect.before_start", &inspected)?;

    agent.start_container(&args.name).await?;
    println!("start: ok");

    tokio::time::sleep(Duration::from_secs(args.wait_secs)).await;
    let http_response = http_get(args.host_port)?;
    println!("http.get:\n{http_response}");

    let logs = agent
        .container_logs(&args.name, &LogsOptions::default())
        .await?;
    println!("logs.text:\n{}", logs.text);

    agent.stop_container(&args.name).await?;
    println!("stop: ok");

    let inspected = agent.inspect_container(&args.name).await?;
    print_json("inspect.after_stop", &inspected)?;

    agent.remove_container(&args.name).await?;
    println!("remove: ok");
    Ok(())
}

fn build_spec(args: &Args) -> ContainerSpec {
    ContainerSpec {
        name: Some(args.name.clone()),
        image: args.image.clone(),
        env: BTreeMap::new(),
        mounts: vec![BindMount {
            host_path: args.mount_host.clone(),
            container_path: args.mount_target.clone(),
        }],
        ports: vec![PortBinding {
            host_port: args.host_port,
            container_port: args.container_port,
            protocol: Default::default(),
        }],
        command: Vec::new(),
        entrypoint: Vec::new(),
        working_dir: None,
        labels: BTreeMap::new(),
    }
}

fn prepare_mount_dir(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(path)?;
    fs::write(
        path.join("index.html"),
        "<html><body><h1>fungi docker smoke</h1></body></html>\n",
    )?;
    Ok(())
}

fn print_json<T: serde::Serialize>(
    label: &str,
    value: &T,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("{label}:\n{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn http_get(port: u16) -> Result<String, Box<dyn std::error::Error>> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    Ok(response)
}

fn resolve_socket_path(explicit: Option<&Path>) -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Some(path) = explicit {
        return Ok(path.to_path_buf());
    }

    if let Ok(host) = env::var("DOCKER_HOST")
        && let Some(path) = host.strip_prefix("unix://")
    {
        return Ok(PathBuf::from(path));
    }

    #[cfg(windows)]
    if let Some(path) = docker_host_named_pipe_path(&host) {
        return Ok(path);
    }

    #[cfg(unix)]
    {
        let home_socket = env::var("HOME")
            .ok()
            .map(PathBuf::from)
            .map(|home| home.join(".docker/run/docker.sock"));
        let candidates = [home_socket, Some(PathBuf::from("/var/run/docker.sock"))];

        for candidate in candidates.into_iter().flatten() {
            if candidate.exists() {
                return Ok(candidate);
            }
        }

        Err("could not detect docker socket; pass --socket explicitly".into())
    }

    #[cfg(windows)]
    {
        Ok(PathBuf::from(r"\\.\pipe\docker_engine"))
    }
}

#[cfg(windows)]
fn docker_host_named_pipe_path(host: &str) -> Option<PathBuf> {
    let raw = host.strip_prefix("npipe://")?;
    let normalized = raw.trim_start_matches('/').replace('/', "\\");
    if normalized.starts_with(r"\\.\pipe\") {
        return Some(PathBuf::from(normalized));
    }
    if normalized.starts_with(r".\pipe\") {
        return Some(PathBuf::from(format!(r"\\{}", normalized)));
    }
    Some(PathBuf::from(normalized))
}
