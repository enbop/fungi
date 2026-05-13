use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const LOCAL_SERVICE: &str = "lab-local-apply-a";
const REMOTE_SERVICE: &str = "lab-remote-apply-b";

fn main() -> Result<()> {
    env_logger::init();

    let repo = workspace_root()?;
    let fungi_bin = sibling_binary("fungi")?;
    let fungi_lab_bin = sibling_binary("fungi-lab")?;
    let manifests_dir = repo.join("target/test-service-apply-cli");
    fs::create_dir_all(&manifests_dir)?;

    let local_v116 = write_manifest(
        &manifests_dir,
        "local-v116.yaml",
        "code-server",
        "http",
        "ghcr.io/coder/code-server:4.116.0",
        "/workspace",
    )?;
    let local_v117 = write_manifest(
        &manifests_dir,
        "local-v117.yaml",
        "code-server",
        "http",
        "ghcr.io/coder/code-server:4.117.0",
        "/home/coder/project",
    )?;
    let local_v117_web = write_manifest(
        &manifests_dir,
        "local-v117-web.yaml",
        "code-server",
        "web",
        "ghcr.io/coder/code-server:4.117.0",
        "/home/coder/project",
    )?;

    let _cleanup = CleanupGuard {
        fungi_bin: fungi_bin.clone(),
        fungi_lab_bin: fungi_lab_bin.clone(),
        repo: repo.clone(),
    };

    docker_cleanup([LOCAL_SERVICE, REMOTE_SERVICE]);
    run_lab(&fungi_lab_bin, &repo, ["clean"])?;
    run_lab(
        &fungi_lab_bin,
        &repo,
        ["start", "--ttl-secs", "1800", "--trust", "both"],
    )?;

    let node_a = repo.join("target/tmp_a");
    let node_b = repo.join("target/tmp_b");

    println!("\n=== Local apply lifecycle ===");
    add_service(&fungi_bin, &node_a, LOCAL_SERVICE, &local_v116)?;
    assert_service(&fungi_bin, &node_a, LOCAL_SERVICE, false, &["http"])?;
    start_service(&fungi_bin, &node_a, LOCAL_SERVICE)?;
    add_service(&fungi_bin, &node_a, LOCAL_SERVICE, &local_v117)?;
    assert_service(&fungi_bin, &node_a, LOCAL_SERVICE, true, &["http"])?;
    add_service(&fungi_bin, &node_a, LOCAL_SERVICE, &local_v117)?;
    assert_service(&fungi_bin, &node_a, LOCAL_SERVICE, true, &["http"])?;

    println!("\n=== Local stopped update and entry replacement ===");
    stop_service(&fungi_bin, &node_a, LOCAL_SERVICE)?;
    add_service(&fungi_bin, &node_a, LOCAL_SERVICE, &local_v116)?;
    assert_service(&fungi_bin, &node_a, LOCAL_SERVICE, false, &["http"])?;
    start_service(&fungi_bin, &node_a, LOCAL_SERVICE)?;
    add_service(&fungi_bin, &node_a, LOCAL_SERVICE, &local_v117_web)?;
    assert_service(&fungi_bin, &node_a, LOCAL_SERVICE, true, &["web"])?;
    add_service(&fungi_bin, &node_a, LOCAL_SERVICE, &local_v117)?;
    assert_service(&fungi_bin, &node_a, LOCAL_SERVICE, true, &["http"])?;

    println!("\n=== Remote apply lifecycle ===");
    add_service(
        &fungi_bin,
        &node_a,
        &format!("{REMOTE_SERVICE}@b"),
        &local_v116,
    )?;
    assert_service(&fungi_bin, &node_b, REMOTE_SERVICE, false, &["http"])?;
    start_service(&fungi_bin, &node_a, &format!("{REMOTE_SERVICE}@b"))?;
    add_service(
        &fungi_bin,
        &node_a,
        &format!("{REMOTE_SERVICE}@b"),
        &local_v117,
    )?;
    assert_service(&fungi_bin, &node_b, REMOTE_SERVICE, true, &["http"])?;
    add_service(
        &fungi_bin,
        &node_a,
        &format!("{REMOTE_SERVICE}@b"),
        &local_v117,
    )?;
    assert_service(&fungi_bin, &node_b, REMOTE_SERVICE, true, &["http"])?;

    println!("\n=== Remote stopped update and entry replacement ===");
    stop_service(&fungi_bin, &node_a, &format!("{REMOTE_SERVICE}@b"))?;
    add_service(
        &fungi_bin,
        &node_a,
        &format!("{REMOTE_SERVICE}@b"),
        &local_v116,
    )?;
    assert_service(&fungi_bin, &node_b, REMOTE_SERVICE, false, &["http"])?;
    start_service(&fungi_bin, &node_a, &format!("{REMOTE_SERVICE}@b"))?;
    add_service(
        &fungi_bin,
        &node_a,
        &format!("{REMOTE_SERVICE}@b"),
        &local_v117_web,
    )?;
    assert_service(&fungi_bin, &node_b, REMOTE_SERVICE, true, &["web"])?;
    add_service(
        &fungi_bin,
        &node_a,
        &format!("{REMOTE_SERVICE}@b"),
        &local_v117,
    )?;
    assert_service(&fungi_bin, &node_b, REMOTE_SERVICE, true, &["http"])?;

    println!("\nAll service apply lab checks passed.");
    Ok(())
}

struct CleanupGuard {
    fungi_bin: PathBuf,
    fungi_lab_bin: PathBuf,
    repo: PathBuf,
}

impl Drop for CleanupGuard {
    fn drop(&mut self) {
        docker_cleanup([LOCAL_SERVICE, REMOTE_SERVICE]);
        let _ = run_cli(
            &self.fungi_bin,
            &self.repo.join("target/tmp_a"),
            ["service", "remove", LOCAL_SERVICE, "--yes"],
        );
        let _ = run_cli(
            &self.fungi_bin,
            &self.repo.join("target/tmp_a"),
            ["service", "remove", &format!("{REMOTE_SERVICE}@b"), "--yes"],
        );
        let _ = run_lab(&self.fungi_lab_bin, &self.repo, ["stop"]);
        let _ = run_lab(&self.fungi_lab_bin, &self.repo, ["clean"]);
    }
}

fn workspace_root() -> Result<PathBuf> {
    let current = std::env::current_dir()?;
    for path in current.ancestors() {
        if path.join("crates").is_dir()
            && path.join("fungi").is_dir()
            && path.join("Cargo.toml").exists()
        {
            return Ok(path.to_path_buf());
        }
    }
    bail!("failed to locate fungi workspace root")
}

fn sibling_binary(name: &str) -> Result<PathBuf> {
    let current_exe = std::env::current_exe().context("failed to locate current executable")?;
    let target_dir = current_exe
        .parent()
        .context("failed to locate executable directory")?;
    let path = target_dir.join(name);
    if !path.exists() {
        bail!("required binary not found at {}", path.display());
    }
    Ok(path)
}

fn unique_suffix() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string()
}

fn write_manifest(
    dir: &Path,
    file_name: &str,
    service_name: &str,
    entry_name: &str,
    image: &str,
    workspace_path: &str,
) -> Result<PathBuf> {
    let path = dir.join(format!("{}-{}", unique_suffix(), file_name));
    let content = format!(
        "apiVersion: fungi.rs/v1alpha1\nkind: Service\n\nmetadata:\n  name: {service_name}\n\nspec:\n  run:\n    docker:\n      image: {image}\n\n  entries:\n    {entry_name}:\n      port: 8080\n      usage: web\n      path: /\n\n  mounts:\n    - hostPath: ${{USER_HOME}}\n      runtimePath: {workspace_path}\n\n  command:\n    - --auth\n    - none\n    - {workspace_path}\n",
    );
    fs::write(&path, content).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(path)
}

fn docker_cleanup<const N: usize>(names: [&str; N]) {
    if Command::new("docker").arg("version").output().is_err() {
        return;
    }
    let _ = Command::new("docker")
        .arg("rm")
        .arg("-f")
        .args(names)
        .output();
}

fn run_lab<I, S>(fungi_lab_bin: &Path, repo: &Path, args: I) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let output = Command::new(fungi_lab_bin)
        .args(
            args.into_iter()
                .map(|value| value.as_ref().to_string())
                .collect::<Vec<_>>(),
        )
        .current_dir(repo)
        .output()
        .context("failed to execute fungi-lab command")?;
    if !output.status.success() {
        bail!(
            "fungi-lab command failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !stdout.is_empty() {
        println!("{stdout}");
    }
    Ok(stdout)
}

fn run_cli<I, S>(fungi_bin: &Path, fungi_dir: &Path, args: I) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let arg_list = args
        .into_iter()
        .map(|value| value.as_ref().to_string())
        .collect::<Vec<_>>();
    let output = Command::new(fungi_bin)
        .arg("--fungi-dir")
        .arg(fungi_dir)
        .args(&arg_list)
        .output()
        .with_context(|| format!("failed to run fungi command {:?}", arg_list))?;
    if !output.status.success() {
        bail!(
            "fungi command {:?} failed\nstdout:\n{}\nstderr:\n{}",
            arg_list,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !stdout.is_empty() {
        println!("{stdout}");
    }
    Ok(stdout)
}

fn add_service(fungi_bin: &Path, fungi_dir: &Path, target: &str, manifest: &Path) -> Result<()> {
    let (name, device) = target
        .split_once('@')
        .map(|(name, device)| (name, Some(device)))
        .unwrap_or((target, None));
    let manifest = manifest
        .to_str()
        .context("manifest path is not valid utf-8")?;
    let mut args = vec!["service", "apply", "--yes", "--name", name];
    if let Some(device) = device {
        args.push("--device");
        args.push(device);
    }
    args.push(manifest);
    let output = run_cli(fungi_bin, fungi_dir, args)?;
    if !output.contains("Remote service applied:") && !output.contains("\"name\":") {
        bail!("unexpected apply output:\n{output}");
    }
    Ok(())
}

fn start_service(fungi_bin: &Path, fungi_dir: &Path, target: &str) -> Result<()> {
    let output = run_cli(fungi_bin, fungi_dir, ["service", "start", target])?;
    if !output.contains("Service started") && !output.contains("Remote service started:") {
        bail!("unexpected start output:\n{output}");
    }
    Ok(())
}

fn stop_service(fungi_bin: &Path, fungi_dir: &Path, target: &str) -> Result<()> {
    let output = run_cli(fungi_bin, fungi_dir, ["service", "stop", target])?;
    if !output.contains("Service stopped") && !output.contains("Remote service stopped:") {
        bail!("unexpected stop output:\n{output}");
    }
    Ok(())
}

fn assert_service(
    fungi_bin: &Path,
    fungi_dir: &Path,
    service: &str,
    expected_running: bool,
    expected_entries: &[&str],
) -> Result<()> {
    let inspect = run_cli(fungi_bin, fungi_dir, ["service", "inspect", service])?;
    let value: Value = serde_json::from_str(&inspect)
        .with_context(|| format!("failed to parse inspect output: {inspect}"))?;

    let running = value
        .get("running")
        .and_then(Value::as_bool)
        .context("inspect output missing running flag")?;
    if running != expected_running {
        bail!(
            "service {} running mismatch: expected {}, got {}\n{}",
            service,
            expected_running,
            running,
            inspect
        );
    }

    let entries = value
        .get("entries")
        .and_then(Value::as_array)
        .context("inspect output missing entries")?
        .iter()
        .filter_map(|entry| entry.get("name").and_then(Value::as_str))
        .collect::<Vec<_>>();

    if entries != expected_entries {
        bail!(
            "service {} entries mismatch: expected {:?}, got {:?}\n{}",
            service,
            expected_entries,
            entries,
            inspect
        );
    }

    let published_entries = value
        .get("published_entries")
        .and_then(Value::as_array)
        .context("inspect output missing published_entries")?
        .iter()
        .filter_map(|entry| entry.get("name").and_then(Value::as_str))
        .collect::<Vec<_>>();

    if published_entries != expected_entries {
        bail!(
            "service {} published entries mismatch: expected {:?}, got {:?}\n{}",
            service,
            expected_entries,
            published_entries,
            inspect
        );
    }

    Ok(())
}
