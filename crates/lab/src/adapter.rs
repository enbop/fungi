//! The small set of Fungi CLI/config/log conventions used by the lab.
use crate::process::ChildGuard;
use anyhow::{Context, Result, bail};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub(crate) fn cli(
    bin: &Path,
    dir: &Path,
    args: &[&str],
    input: Option<&str>,
    deadline: Instant,
) -> Result<String> {
    let mut command = Command::new(bin);
    command.arg("--fungi-dir").arg(dir).args(args);
    capture(&mut command, input, deadline)
}

fn capture(command: &mut Command, input: Option<&str>, deadline: Instant) -> Result<String> {
    if Instant::now() >= deadline {
        bail!("lab operation timed out");
    }
    // Temporary files avoid pipe deadlocks when a child produces a lot of output.
    let mut stdout = tempfile::tempfile()?;
    let mut stderr = tempfile::tempfile()?;
    command
        .stdout(stdout.try_clone()?)
        .stderr(stderr.try_clone()?);
    command.stdin(if input.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    });
    let mut guard = ChildGuard::spawn(command)?;
    if let Some(input) = input {
        guard
            .child()
            .stdin
            .take()
            .context("missing command stdin")?
            .write_all(input.as_bytes())?;
    }
    let status = loop {
        if let Some(status) = guard.child().try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            bail!("fungi command timed out: {command:?}");
        }
        thread::sleep(Duration::from_millis(50));
    };
    stdout.rewind()?;
    stderr.rewind()?;
    let mut out = String::new();
    let mut err = String::new();
    stdout.read_to_string(&mut out)?;
    stderr.read_to_string(&mut err)?;
    if !status.success() {
        bail!("fungi command failed ({status}): {command:?}\n{out}\n{err}");
    }
    Ok(out.trim().to_string())
}

pub(crate) fn peer_id(text: &str) -> Option<String> {
    text.split_whitespace()
        .find(|part| part.starts_with("16Uiu"))
        .map(str::to_string)
}

pub(crate) fn wait_node(
    bin: &Path,
    dir: &Path,
    child: &mut ChildGuard,
    deadline: Instant,
) -> Result<String> {
    let mut last = String::new();
    while Instant::now() < deadline {
        child.ensure_running()?;
        match cli(
            bin,
            dir,
            &["info", "id"],
            None,
            deadline.min(Instant::now() + Duration::from_secs(3)),
        ) {
            Ok(text) => {
                if let Some(id) = peer_id(&text) {
                    return Ok(id);
                }
            }
            Err(error) => last = format!("{error:#}"),
        }
        thread::sleep(Duration::from_millis(100));
    }
    bail!("timed out waiting for node at {}\n{last}", dir.display())
}

// Retained for the shared CLI-test helpers.
pub(crate) fn wait_ready_with_bin(
    bin: &Path,
    _repo: &Path,
    dir: &Path,
    timeout: Duration,
) -> Result<()> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if cli(
            bin,
            dir,
            &["info", "version"],
            None,
            deadline.min(Instant::now() + Duration::from_secs(3)),
        )
        .is_ok()
        {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    bail!("daemon did not become ready at {}", dir.display())
}

pub(crate) fn wait_relay(
    log: &Path,
    offset: u64,
    child: &mut ChildGuard,
    deadline: Instant,
) -> Result<String> {
    let mut last = String::new();
    while Instant::now() < deadline {
        child
            .ensure_running()
            .with_context(|| format!("relay startup failed; log: {}\n{last}", log.display()))?;
        let mut file = File::open(log)?;
        file.seek(SeekFrom::Start(offset))?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        if contents.contains("Added external addresses:")
            && let Some(id) = contents
                .lines()
                .find_map(|line| line.trim().strip_prefix("Local peer id: "))
        {
            return Ok(id.to_string());
        }
        last = contents
            .lines()
            .rev()
            .take(20)
            .collect::<Vec<_>>()
            .join("\n");
        thread::sleep(Duration::from_millis(100));
    }
    bail!(
        "timed out waiting for relay; log: {}\n{last}",
        log.display()
    )
}

pub(crate) fn configure_node(dir: &Path, relays: &[String]) -> Result<()> {
    let path = dir.join("config.toml");
    let mut config: toml::Value = toml::from_str(&fs::read_to_string(&path)?)?;
    let table = config
        .as_table_mut()
        .context("config must be a TOML table")?;
    let rpc = table
        .entry("rpc")
        .or_insert_with(|| toml::Value::Table(Default::default()))
        .as_table_mut()
        .context("rpc must be a TOML table")?;
    rpc.insert("listen_address".into(), "127.0.0.1:0".into());
    let network = table
        .entry("network")
        .or_insert_with(|| toml::Value::Table(Default::default()))
        .as_table_mut()
        .context("network must be a TOML table")?;
    network.insert("listen_tcp_port".into(), 0.into());
    network.insert("listen_udp_port".into(), 0.into());
    network.insert("relay_enabled".into(), true.into());
    network.insert("use_community_relays".into(), false.into());
    network.insert(
        "custom_relay_addresses".into(),
        toml::Value::Array(relays.iter().cloned().map(toml::Value::String).collect()),
    );
    fs::write(path, toml::to_string_pretty(&config)?)?;
    Ok(())
}

pub(crate) fn open_log(path: &Path) -> Result<File> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    writeln!(file, "\n--- lab process start at {now} ---")?;
    Ok(file)
}

pub(crate) fn quote(text: impl AsRef<str>) -> String {
    format!("'{}'", text.as_ref().replace('\'', "'\"'\"'"))
}
