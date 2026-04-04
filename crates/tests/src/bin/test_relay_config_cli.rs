use anyhow::{Context, Result, bail};
use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

const CUSTOM_RELAY: &str =
    "/ip4/127.0.0.1/tcp/39001/p2p/16Uiu2HAmGXFS6aYsKKYRkEDo1tNigZKN8TAYrsfSnEdC5sZLNkiE";

fn main() -> Result<()> {
    let ctx = RelayTestContext::new()?;

    println!("=== Offline relay config ===");
    let output = ctx.run_cli(["relay", "disable"])?;
    assert_contains(&output, "Relay disabled")?;

    let offline_show = ctx.run_cli(["relay", "show"])?;
    assert_contains(&offline_show, "relay_enabled: false")?;
    assert_contains(&offline_show, "effective_relay_addresses:")?;
    assert_contains(&offline_show, "  <none>")?;

    let config_text = fs::read_to_string(ctx.config_file())?;
    assert_contains(&config_text, "relay_enabled = false")?;

    let rpc_port = find_free_port()?;
    ctx.set_rpc_port(rpc_port)?;

    println!("=== Start daemon ===");
    let daemon = DaemonProcess::start(ctx.fungi_dir())?;

    println!("=== Online relay config via daemon ===");
    let output = ctx.run_cli(["relay", "enable"])?;
    assert_contains(&output, "Relay enabled")?;
    assert_contains(&output, "Restart daemon to fully apply changes")?;

    let output = ctx.run_cli(["relay", "use-community", "off"])?;
    assert_contains(&output, "Community relay disabled")?;

    let output = ctx.run_cli(["relay", "add", CUSTOM_RELAY])?;
    assert_contains(&output, "Custom relay added")?;

    println!("=== Mutate unrelated config through daemon ===");
    let output = ctx.run_cli(["security", "allow-port", "19001"])?;
    assert_contains(&output, "Allowed port added")?;

    println!("=== Verify relay config survived ===");
    let show = ctx.run_cli(["relay", "show"])?;
    assert_contains(&show, "relay_enabled: true")?;
    assert_contains(&show, "use_community_relays: false")?;
    assert_contains(&show, CUSTOM_RELAY)?;
    assert_contains(&show, "[custom]")?;

    let config_text = fs::read_to_string(ctx.config_file())?;
    assert_contains(&config_text, "relay_enabled = true")?;
    assert_contains(&config_text, "use_community_relays = false")?;
    assert_contains(&config_text, CUSTOM_RELAY)?;
    assert_contains(&config_text, "19001")?;

    drop(daemon);
    println!("Relay CLI smoke test passed.");
    Ok(())
}

struct RelayTestContext {
    _temp_dir: TempDir,
    fungi_dir: PathBuf,
}

impl RelayTestContext {
    fn new() -> Result<Self> {
        let temp_dir = TempDir::new().context("failed to create temp dir")?;
        let fungi_dir = temp_dir.path().join("fungi-home");
        fs::create_dir_all(&fungi_dir).context("failed to create fungi dir")?;
        Ok(Self {
            _temp_dir: temp_dir,
            fungi_dir,
        })
    }

    fn fungi_dir(&self) -> &Path {
        &self.fungi_dir
    }

    fn config_file(&self) -> PathBuf {
        self.fungi_dir.join("config.toml")
    }

    fn set_rpc_port(&self, port: u16) -> Result<()> {
        let config_file = self.config_file();
        let content = fs::read_to_string(&config_file)
            .with_context(|| format!("failed to read {}", config_file.display()))?;
        let updated = content.replace(
            "listen_address = \"127.0.0.1:5405\"",
            &format!("listen_address = \"127.0.0.1:{port}\""),
        );
        fs::write(&config_file, updated)
            .with_context(|| format!("failed to write {}", config_file.display()))?;
        Ok(())
    }

    fn run_cli<I, S>(&self, args: I) -> Result<String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let fungi_bin = get_fungi_binary_path()?;
        let arg_list = args
            .into_iter()
            .map(|entry| entry.as_ref().to_string())
            .collect::<Vec<_>>();

        let output = Command::new(&fungi_bin)
            .arg("--fungi-dir")
            .arg(&self.fungi_dir)
            .args(&arg_list)
            .output()
            .with_context(|| format!("failed to run cli command {:?}", arg_list))?;

        if !output.status.success() {
            bail!(
                "command {:?} failed\nstdout:\n{}\nstderr:\n{}",
                arg_list,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            );
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}

struct DaemonProcess {
    child: Child,
}

impl DaemonProcess {
    fn start(fungi_dir: &Path) -> Result<Self> {
        let fungi_bin = get_fungi_binary_path()?;
        let child = Command::new(&fungi_bin)
            .arg("--fungi-dir")
            .arg(fungi_dir)
            .arg("daemon")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("failed to start daemon")?;

        wait_ready(fungi_dir, Duration::from_secs(20))?;
        Ok(Self { child })
    }
}

impl Drop for DaemonProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn wait_ready(fungi_dir: &Path, timeout: Duration) -> Result<()> {
    let fungi_bin = get_fungi_binary_path()?;
    let started = Instant::now();

    while started.elapsed() < timeout {
        let output = Command::new(&fungi_bin)
            .arg("--fungi-dir")
            .arg(fungi_dir)
            .arg("info")
            .arg("version")
            .output()
            .context("failed to probe daemon readiness")?;

        if output.status.success() {
            return Ok(());
        }

        thread::sleep(Duration::from_millis(300));
    }

    bail!("daemon did not become ready within {:?}", timeout)
}

fn get_fungi_binary_path() -> Result<PathBuf> {
    let current_exe = std::env::current_exe().context("failed to get current executable")?;
    let target_dir = current_exe
        .parent()
        .context("failed to get current executable directory")?;
    let fungi_bin = target_dir.join("fungi");

    if !fungi_bin.exists() {
        bail!(
            "fungi binary not found at {}. Build it first with `cargo build --bin fungi`.",
            fungi_bin.display()
        );
    }

    Ok(fungi_bin)
}

fn assert_contains(haystack: &str, needle: &str) -> Result<()> {
    if haystack.contains(needle) {
        return Ok(());
    }

    bail!("expected to find `{needle}` in:\n{haystack}")
}

fn find_free_port() -> Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).context("failed to bind probe port")?;
    Ok(listener.local_addr()?.port())
}
