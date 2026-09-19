use super::*;
use state::{Lab, STATE_FILE, Target, TrustMode, lock_lab};
use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

fn fixture(root: &Path) -> Lab {
    Lab::new(root, std::env::current_exe().unwrap(), 12345, 12346)
}

#[test]
fn state_writes_are_atomic_and_reject_old_or_corrupt_data() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let _lock = lock_lab(root, true).unwrap();
    let lab = fixture(root);
    lab.save().unwrap();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            for _ in 0..50 {
                lab.save().unwrap();
            }
        });
        for _ in 0..50 {
            Lab::load(root).unwrap();
        }
    });
    let mut value = serde_json::to_value(&lab.state).unwrap();
    value["version"] = 2.into();
    fs::write(root.join(STATE_FILE), value.to_string()).unwrap();
    assert!(
        Lab::load(root)
            .err()
            .unwrap()
            .to_string()
            .contains("unsupported lab state version")
    );
    fs::write(root.join(STATE_FILE), "broken").unwrap();
    assert!(runtime::clean(root).is_err());
    assert_eq!(fs::read_to_string(root.join(STATE_FILE)).unwrap(), "broken");
}

#[test]
fn mutating_commands_are_exclusive_and_unowned_data_is_preserved() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("keep"), "user data").unwrap();
    assert!(lock_lab(temp.path(), true).is_err());
    assert!(runtime::clean(temp.path()).is_err());
    assert_eq!(
        fs::read_to_string(temp.path().join("keep")).unwrap(),
        "user data"
    );
    let root = temp.path().join("lab");
    let lock = lock_lab(&root, true).unwrap();
    fixture(&root).save().unwrap();
    Lab::load(&root).expect("the ownership marker must remain readable while locked");
    assert!(
        lock_lab(&root, false)
            .unwrap_err()
            .to_string()
            .contains("another command is managing this lab")
    );
    drop(lock);
    assert!(lock_lab(&root, false).is_ok());
    let root = temp.path().join("failed-start");
    let _lock = lock_lab(&root, true).unwrap();
    let missing_bin = temp.path().join("missing-fungi");
    assert!(
        runtime::start(
            &root,
            cli::StartArgs {
                fungi_bin: Some(missing_bin),
                trust: TrustMode::None
            }
        )
        .is_err()
    );
    runtime::clean(&root).unwrap();
    assert!(!root.exists());
}

#[cfg(unix)]
#[test]
fn symlinked_lock_is_rejected_without_touching_its_target() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("lab");
    drop(lock_lab(&root, true).unwrap());
    let target = temp.path().join("keep");
    fs::write(&target, "user data").unwrap();
    fs::remove_file(root.join(state::LOCK_FILE)).unwrap();
    std::os::unix::fs::symlink(&target, root.join(state::LOCK_FILE)).unwrap();
    assert!(lock_lab(&root, false).is_err());
    assert!(runtime::clean(&root).is_err());
    assert_eq!(fs::read_to_string(target).unwrap(), "user data");
}

#[cfg(unix)]
#[test]
fn symlinked_lab_storage_is_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("target");
    fs::create_dir(&target).unwrap();
    let alias = temp.path().join("alias");
    std::os::unix::fs::symlink(&target, &alias).unwrap();
    assert!(lock_lab(&alias, true).is_err());
    assert!(fs::read_dir(&target).unwrap().next().is_none());
    let root = temp.path().join("lab");
    let _lock = lock_lab(&root, true).unwrap();
    fixture(&root).save().unwrap();
    std::os::unix::fs::symlink(&target, root.join("nodes")).unwrap();
    assert!(runtime::clean(&root).is_err());
    assert!(root.exists());
}

#[test]
fn process_identity_mismatch_never_signals_the_process() {
    let id: process::ProcessId =
        serde_json::from_value(serde_json::json!({"pid":std::process::id(), "started_at":0}))
            .unwrap();
    assert!(id.stop(&std::env::current_exe().unwrap(), &[]).is_err());
}

#[cfg(target_os = "linux")]
#[test]
fn replacing_a_binary_does_not_lose_ownership_of_its_running_process() {
    let temp = tempfile::tempdir().unwrap();
    let bin = temp.path().join("test-process");
    fs::copy("/bin/sleep", &bin).unwrap();
    let mut command = Command::new(&bin);
    command
        .arg("60")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = process::ChildGuard::spawn(&mut command).unwrap();
    let id = process::ProcessId::capture(child.child().id()).unwrap();
    let args = ["60".into()];
    assert!(id.running(&bin, &args).unwrap());
    fs::remove_file(&bin).unwrap();
    fs::copy("/bin/sleep", &bin).unwrap();
    assert!(id.running(&bin, &args).unwrap());
    id.stop(&bin, &args).unwrap();
    child.stop().unwrap();
    assert_no_process(id.pid);
}

#[test]
fn dynamic_node_config_preserves_other_settings() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("config.toml"), "version = 3\n[rpc]\nlisten_address='127.0.0.1:1234'\n[network]\nlisten_tcp_port=2345\nlisten_udp_port=3456\n[runtime]\ndisable_docker=true\nallowed_host_paths=['/example']\n").unwrap();
    adapter::configure_node(temp.path(), &["/ip4/127.0.0.1/tcp/4567/p2p/peer".into()]).unwrap();
    let cfg: toml::Value =
        toml::from_str(&fs::read_to_string(temp.path().join("config.toml")).unwrap()).unwrap();
    assert_eq!(cfg["rpc"]["listen_address"].as_str(), Some("127.0.0.1:0"));
    assert_eq!(cfg["network"]["listen_tcp_port"].as_integer(), Some(0));
    assert_eq!(cfg["network"]["listen_udp_port"].as_integer(), Some(0));
    assert_eq!(
        cfg["network"]["use_community_relays"].as_bool(),
        Some(false)
    );
    assert_eq!(cfg["runtime"]["disable_docker"].as_bool(), Some(true));
    assert_eq!(
        cfg["runtime"]["allowed_host_paths"][0].as_str(),
        Some("/example")
    );
}

fn assert_no_process(pid: u32) {
    let system = sysinfo::System::new_all();
    assert!(
        system
            .process(sysinfo::Pid::from_u32(pid))
            .is_none_or(|p| p.status() == sysinfo::ProcessStatus::Zombie),
        "pid {pid} still running"
    );
}

#[cfg(unix)]
#[test]
fn startup_timeout_reclaims_the_child_and_retains_logs() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("lab");
    let _lock = lock_lab(&root, true).unwrap();
    let binary = temp.path().join("slow-fungi");
    fs::write(&binary, "#!/bin/sh\necho test-pid=$$\nexec sleep 60\n").unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
    let mut lab = Lab::new(&root, binary, 12345, 12346);
    lab.save().unwrap();
    let error = lab
        .start_targets(
            &[Target::Relay],
            None,
            Instant::now() + Duration::from_millis(300),
        )
        .unwrap_err();
    assert!(format!("{error:#}").contains("timed out"), "{error:#}");
    assert!(
        format!("{error:#}").contains("startup rollback completed"),
        "{error:#}"
    );
    let log = fs::read_to_string(lab.log(Target::Relay)).unwrap();
    let pid: u32 = log
        .lines()
        .find_map(|line| line.strip_prefix("test-pid="))
        .unwrap()
        .parse()
        .unwrap();
    assert_no_process(pid);
    assert!(Lab::load(&root).unwrap().state.relay.process.is_none());
}

struct RealLab {
    lab: Lab,
    _lock: fs::File,
    _temp: tempfile::TempDir,
}
impl RealLab {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let lock = lock_lab(&root, true).unwrap();
        let bin = state::find_repo_root().unwrap().join("target/debug/fungi");
        assert!(bin.exists(), "build fungi first");
        let lab = Lab::new(
            &root,
            bin,
            support::reserve_tcp_port().unwrap(),
            support::reserve_udp_port().unwrap(),
        );
        lab.save().unwrap();
        Self {
            lab,
            _lock: lock,
            _temp: temp,
        }
    }
}
impl Drop for RealLab {
    fn drop(&mut self) {
        self.lab
            .stop(&[Target::A, Target::B, Target::Relay])
            .expect("test lab cleanup failed");
    }
}

#[test]
#[ignore = "requires built fungi and local sockets"]
fn real_partial_startup_and_restart_failures_are_scoped() {
    for node in ["a", "b"] {
        let mut fixture = RealLab::new();
        let lab = &mut fixture.lab;
        let parent = lab.root.join("nodes").join(node);
        fs::create_dir_all(&parent).unwrap();
        fs::write(parent.join("fungi"), "injected directory failure").unwrap();
        let error = lab
            .start_targets(
                &[Target::Relay, Target::A, Target::B],
                Some(TrustMode::None),
                Instant::now() + runtime::STARTUP_TIMEOUT,
            )
            .unwrap_err();
        assert!(
            format!("{error:#}").contains("startup rollback completed"),
            "{error:#}"
        );
        // Scan actual command lines as well as the cleared state.
        for process in sysinfo::System::new_all().processes().values() {
            assert!(
                process.status() == sysinfo::ProcessStatus::Zombie
                    || !process
                        .cmd()
                        .iter()
                        .skip(1)
                        .eq(lab.args(Target::Relay).iter())
                    || process.exe() != Some(lab.state.fungi_bin.as_path())
            );
            assert!(
                process.status() == sysinfo::ProcessStatus::Zombie
                    || !process.cmd().iter().any(|arg| arg
                        .to_string_lossy()
                        .starts_with(lab.root.to_string_lossy().as_ref()))
            );
        }
    }
    let mut fixture = RealLab::new();
    let lab = &mut fixture.lab;
    lab.start_targets(
        &[Target::Relay, Target::A, Target::B],
        Some(TrustMode::None),
        Instant::now() + runtime::STARTUP_TIMEOUT,
    )
    .unwrap();
    let b = lab.state.node_b.process.unwrap();
    let relay = lab.state.relay.process.unwrap();
    lab.stop(&[Target::A]).unwrap();
    fs::write(lab.dir(Target::A).join("config.toml"), "invalid TOML [").unwrap();
    assert!(lab.manage(Target::A, state::ProcessCommand::Start).is_err());
    assert!(
        b.running(&lab.state.fungi_bin, &lab.args(Target::B))
            .unwrap()
    );
    assert!(
        relay
            .running(&lab.state.fungi_bin, &lab.args(Target::Relay))
            .unwrap()
    );
}

#[test]
#[ignore = "requires built fungi/fungi-lab and local sockets"]
fn real_cli_persists_nodes_and_discovers_dynamic_rpc_after_restart() {
    struct Cleanup(std::path::PathBuf, std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let status = Command::new(&self.0)
                .arg("--lab-dir")
                .arg(&self.1)
                .arg("clean")
                .stdout(Stdio::null())
                .status()
                .unwrap();
            assert!(status.success(), "CLI lab cleanup failed");
        }
    }
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("lab");
    let repo = state::find_repo_root().unwrap();
    let binary = repo.join("target/debug/fungi-lab");
    let call = |args: &[&str]| {
        let out = Command::new(&binary)
            .arg("--lab-dir")
            .arg(&root)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    call(&["start"]);
    let _cleanup = Cleanup(binary.clone(), root.clone());
    let mut lab = Lab::load(&root).unwrap();
    let fungi = |target, args: &[&str]| {
        adapter::cli(
            &lab.state.fungi_bin,
            &lab.dir(target),
            args,
            None,
            Instant::now() + Duration::from_secs(15),
        )
        .unwrap()
    };
    for target in [Target::A, Target::B] {
        assert!(lab.running(target).unwrap());
        assert!(fungi(target, &["device", "trusted"]).contains("No trusted devices"));
        let cfg: toml::Value =
            toml::from_str(&fs::read_to_string(lab.dir(target).join("config.toml")).unwrap())
                .unwrap();
        assert_eq!(cfg["rpc"]["listen_address"].as_str(), Some("127.0.0.1:0"));
        assert!(!fungi(target, &["info", "rpc-address"]).ends_with(":0"));
    }
    let original_state = fs::read_to_string(root.join(STATE_FILE)).unwrap();
    assert!(!original_state.contains("manager") && !original_state.contains("rpc_port"));
    for (mode, a, b) in [
        ("both", true, true),
        ("b-trusts-a", false, true),
        ("a-trusts-b", true, false),
        ("none", false, false),
    ] {
        call(&["trust", mode]);
        assert_eq!(
            fungi(Target::A, &["device", "trusted"]).contains(&lab.state.node_b.peer_id),
            a
        );
        assert_eq!(
            fungi(Target::B, &["device", "trusted"]).contains(&lab.state.node_a.peer_id),
            b
        );
    }
    let a = lab.state.node_a.process.unwrap();
    let peer = lab.state.node_a.peer_id.clone();
    let relay_addresses = lab.relay_addresses();
    let old_log = fs::read(lab.log(Target::A)).unwrap();
    let old_relay_log = fs::read(lab.log(Target::Relay)).unwrap();
    call(&["node", "restart", "a"]);
    call(&["relay", "restart"]);
    assert_no_process(a.pid);
    lab = Lab::load(&root).unwrap();
    assert_eq!(lab.state.node_a.peer_id, peer);
    assert_eq!(lab.relay_addresses(), relay_addresses);
    assert!(fs::read(lab.log(Target::A)).unwrap().starts_with(&old_log));
    assert!(
        fs::read(lab.log(Target::Relay))
            .unwrap()
            .starts_with(&old_relay_log)
    );
    assert_eq!(
        adapter::peer_id(
            &adapter::cli(
                &lab.state.fungi_bin,
                &lab.dir(Target::A),
                &["info", "id"],
                None,
                Instant::now() + Duration::from_secs(5)
            )
            .unwrap()
        )
        .unwrap(),
        peer
    );
    call(&["status", "--json"]);
    call(&["env"]);
    let pids: Vec<_> = [Target::Relay, Target::A, Target::B]
        .map(|t| lab.node(t).process.unwrap().pid)
        .into();
    call(&["stop"]);
    for pid in pids {
        assert_no_process(pid);
    }
    call(&["start"]);
    assert_eq!(Lab::load(&root).unwrap().state.node_a.peer_id, peer);
}
