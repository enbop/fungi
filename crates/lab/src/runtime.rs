use crate::{
    adapter::{self, quote},
    cli::StartArgs,
    process::{ChildGuard, ProcessId},
    state::{Lab, ProcessCommand, STATE_FILE, Target, TrustMode},
    support::{get_fungi_binary_path, reserve_tcp_port, reserve_udp_port},
};
use anyhow::{Context, Result, anyhow, bail};
use serde_json::json;
use std::{
    ffi::OsString,
    fs,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub(crate) const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
const ALL: [Target; 3] = [Target::Relay, Target::A, Target::B];
const STOP_ORDER: [Target; 3] = [Target::B, Target::A, Target::Relay];

pub(crate) fn start(root: &Path, args: StartArgs) -> Result<()> {
    let existing = root.join(STATE_FILE).exists();
    let mut lab = if existing {
        Lab::load(root)?
    } else {
        let bin = args
            .fungi_bin
            .clone()
            .map_or_else(get_fungi_binary_path, Ok)?
            .canonicalize()?;
        Lab::new(root, bin, reserve_tcp_port()?, reserve_udp_port()?)
    };
    for target in ALL {
        if lab.running(target)? {
            bail!(
                "{} is already running; stop this lab before starting it again",
                target.label()
            );
        }
    }
    if let Some(bin) = args.fungi_bin {
        lab.state.fungi_bin = bin.canonicalize()?;
    }
    lab.save()?;
    lab.start_targets(&ALL, Some(args.trust), Instant::now() + STARTUP_TIMEOUT)?;
    println!("Lab started. Processes run until stop/clean; there is no automatic expiry.");
    print_status(&lab, false)
}

impl Lab {
    // These exact arguments also identify processes during later stop/status commands.
    pub(crate) fn args(&self, target: Target) -> Vec<OsString> {
        if target == Target::Relay {
            [
                "daemon",
                "relay-server",
                "--public-ip",
                "127.0.0.1",
                "--tcp-listen-port",
                &self.state.relay_tcp_port.to_string(),
                "--udp-listen-port",
                &self.state.relay_udp_port.to_string(),
            ]
            .into_iter()
            .map(OsString::from)
            .collect()
        } else {
            vec![
                "--fungi-dir".into(),
                self.dir(target).into_os_string(),
                "daemon".into(),
            ]
        }
    }

    pub(crate) fn running(&self, target: Target) -> Result<bool> {
        self.node(target)
            .process
            .map_or(Ok(false), |process| {
                process.running(&self.state.fungi_bin, &self.args(target))
            })
            .with_context(|| format!("cannot identify {}", target.label()))
    }

    pub(crate) fn stop(&mut self, targets: &[Target]) -> Result<()> {
        let mut errors = Vec::new();
        for &target in targets {
            let result = (|| {
                if let Some(process) = self.node(target).process {
                    process.stop(&self.state.fungi_bin, &self.args(target))?;
                }
                self.node_mut(target).process = None;
                self.save()
            })();
            if let Err(error) = result {
                errors.push(format!("{}: {error:#}", target.label()));
            }
        }
        if !errors.is_empty() {
            bail!(
                "some lab processes could not be stopped; state retained:\n{}",
                errors.join("\n")
            );
        }
        Ok(())
    }

    pub(crate) fn manage(&mut self, target: Target, operation: ProcessCommand) -> Result<()> {
        match operation {
            ProcessCommand::Stop => self.stop(&[target])?,
            ProcessCommand::Start if self.running(target)? => {
                println!("{} is already running.", target.label());
            }
            ProcessCommand::Start | ProcessCommand::Restart => {
                if matches!(operation, ProcessCommand::Restart) {
                    self.stop(&[target])?;
                }
                self.start_targets(&[target], None, Instant::now() + STARTUP_TIMEOUT)?;
            }
        }
        Ok(())
    }

    pub(crate) fn start_targets(
        &mut self,
        targets: &[Target],
        trust: Option<TrustMode>,
        deadline: Instant,
    ) -> Result<()> {
        let mut started: Vec<(Target, ChildGuard)> = Vec::new();
        let result = (|| {
            for &target in targets {
                self.launch(target, &mut started, deadline)?;
            }
            if let Some(mode) = trust {
                self.add_devices(deadline)?;
                self.trust(mode, deadline)?;
            }
            self.save()
        })();
        if let Err(error) = result {
            let mut cleanup_errors = Vec::new();
            for (target, child) in started.iter_mut().rev() {
                match child.stop() {
                    Ok(()) => self.node_mut(*target).process = None,
                    Err(error) => cleanup_errors.push(format!("{}: {error:#}", target.label())),
                }
            }
            if let Err(error) = self.save() {
                cleanup_errors.push(format!("state: {error:#}"));
            }
            let outcome = if cleanup_errors.is_empty() {
                "startup rollback completed".to_string()
            } else {
                format!(
                    "rollback incomplete; retain state/logs:\n{}",
                    cleanup_errors.join("\n")
                )
            };
            return Err(anyhow!("{error:#}\n{outcome}"));
        }
        for (_, child) in started {
            child.release();
        }
        Ok(())
    }

    fn launch(
        &mut self,
        target: Target,
        started: &mut Vec<(Target, ChildGuard)>,
        deadline: Instant,
    ) -> Result<()> {
        if Instant::now() >= deadline {
            bail!("lab startup timed out");
        }
        if self.running(target)? {
            bail!("{} is already running", target.label());
        }
        let dir = self.dir(target);
        fs::create_dir_all(&dir).with_context(|| format!("cannot create {}", dir.display()))?;
        if target != Target::Relay {
            // Do not reconfigure or duplicate an unrecorded daemon using this fungi-dir.
            if adapter::cli(
                &self.state.fungi_bin,
                &dir,
                &["info", "id"],
                None,
                deadline.min(Instant::now() + Duration::from_secs(3)),
            )
            .is_ok()
            {
                bail!(
                    "an unrecorded daemon is already using {}; refusing to replace it",
                    dir.display()
                );
            }
            adapter::cli(&self.state.fungi_bin, &dir, &["init"], None, deadline)?;
            adapter::configure_node(&dir, &self.relay_addresses())?;
        }
        let log = self.log(target);
        let output = adapter::open_log(&log)?;
        let offset = output.metadata()?.len();
        let mut command = Command::new(&self.state.fungi_bin);
        command
            .args(self.args(target))
            .stdin(Stdio::null())
            .stdout(output.try_clone()?)
            .stderr(output);
        if target == Target::Relay {
            command.env("HOME", &dir);
        }
        let child = ChildGuard::spawn(&mut command)?;
        started.push((target, child));
        let child = &mut started.last_mut().unwrap().1;
        self.node_mut(target).process = Some(ProcessId::capture(child.child().id())?);
        self.save()?; // Journal before readiness checks; later commands can recover interrupted starts.
        let id = if target == Target::Relay {
            adapter::wait_relay(&log, offset, child, deadline)?
        } else {
            adapter::wait_node(&self.state.fungi_bin, &dir, child, deadline)?
        };
        let previous = &self.node(target).peer_id;
        if !previous.is_empty() && previous != &id {
            bail!(
                "{} identity changed; stop/clean before reusing this lab",
                target.label()
            );
        }
        self.node_mut(target).peer_id = id;
        self.save()?;
        Ok(())
    }

    fn add_devices(&self, deadline: Instant) -> Result<()> {
        let relay = &self.relay_addresses()[0];
        for (node, other, name) in [(Target::A, Target::B, "b"), (Target::B, Target::A, "a")] {
            let peer = &self.node(other).peer_id;
            let address = format!("{relay}/p2p-circuit/p2p/{peer}");
            adapter::cli(
                &self.state.fungi_bin,
                &self.dir(node),
                &["device", "add", name, peer, "--addr", &address],
                None,
                deadline,
            )?;
        }
        Ok(())
    }

    pub(crate) fn trust(&self, mode: TrustMode, deadline: Instant) -> Result<()> {
        for target in [Target::A, Target::B] {
            if !self.running(target)? {
                bail!("both nodes must be running to configure trust");
            }
        }
        for (node, other, grant) in [
            (
                Target::A,
                Target::B,
                matches!(mode, TrustMode::Both | TrustMode::ATrustsB),
            ),
            (
                Target::B,
                Target::A,
                matches!(mode, TrustMode::Both | TrustMode::BTrustsA),
            ),
        ] {
            let dir = self.dir(node);
            let peer = &self.node(other).peer_id;
            if grant {
                println!(
                    "{} ({}) grants service-management access to {peer} until revoked.",
                    node.label(),
                    self.node(node).peer_id
                );
                println!(
                    "{}",
                    adapter::cli(
                        &self.state.fungi_bin,
                        &dir,
                        &["security", "show"],
                        None,
                        deadline
                    )?
                );
                println!(
                    "Rollback: fungi-lab --lab-dir {} trust none",
                    quote(self.root.to_string_lossy())
                );
            }
            adapter::cli(
                &self.state.fungi_bin,
                &dir,
                &["device", if grant { "trust" } else { "untrust" }, peer],
                if grant { Some("y\n") } else { None },
                deadline,
            )
            .context("trust update may be partial; inspect both nodes with device trusted")?;
        }
        println!("Trust mode applied: {mode:?}.");
        Ok(())
    }
}

pub(crate) fn clean(root: &Path) -> Result<()> {
    // Ownership is checked before any process or directory mutation.
    crate::state::validate_layout(root)?;
    let mut protected = vec![std::env::current_exe()?];
    if let Some(home) = std::env::var_os("HOME") {
        protected.push(home.into());
    }
    if root.parent().is_none() || protected.iter().any(|p| p.starts_with(root)) {
        bail!("refusing to remove a broad/protected lab directory");
    }
    // A failed binary lookup can leave only the ownership marker and lock: no process
    // can have been spawned before the initial state was written.
    if fs::read_dir(root)?.all(|entry| {
        entry.is_ok_and(|e| {
            e.file_name() == ".fungi-lab" || e.file_name() == crate::state::LOCK_FILE
        })
    }) {
        fs::remove_dir_all(root)?;
        println!("Removed empty lab directory at {}.", root.display());
        return Ok(());
    }
    let mut lab = Lab::load(root)?;
    if lab.state.fungi_bin.starts_with(root) {
        bail!("lab directory contains its Fungi binary; refusing cleanup");
    }
    lab.stop(&STOP_ORDER)?;
    fs::remove_dir_all(root)?;
    println!("Removed lab processes and data at {}.", root.display());
    Ok(())
}

pub(crate) fn print_status(lab: &Lab, as_json: bool) -> Result<()> {
    let node = |target| -> Result<serde_json::Value> {
        Ok(
            json!({"pid": lab.node(target).process.map(|p| p.pid), "running": lab.running(target)?,
            "peer_id": lab.node(target).peer_id, "dir": lab.dir(target), "log": lab.log(target)}),
        )
    };
    let status = json!({"lab_dir": lab.root, "fungi_bin": lab.state.fungi_bin,
        "relay": node(Target::Relay)?, "relay_addresses": lab.relay_addresses(),
        "node_a": node(Target::A)?, "node_b": node(Target::B)?});
    if as_json {
        println!("{}", serde_json::to_string_pretty(&status)?);
    } else {
        println!("Lab directory: {}", lab.root.display());
        for (target, field) in [
            (Target::Relay, "relay"),
            (Target::A, "node_a"),
            (Target::B, "node_b"),
        ] {
            println!(
                "  {}: {} pid={} peer={}",
                target.label(),
                if status[field]["running"] == true {
                    "running"
                } else {
                    "stopped"
                },
                status[field]["pid"],
                lab.node(target).peer_id
            );
            println!(
                "    dir: {}\n    log: {}",
                lab.dir(target).display(),
                lab.log(target).display()
            );
        }
        println!(
            "Running means process identity matched; use fungi -f DIR info id/ping to check readiness/connectivity."
        );
    }
    Ok(())
}

pub(crate) fn print_env(lab: &Lab) -> Result<()> {
    for (name, value) in [
        ("FUNGI_BIN", lab.state.fungi_bin.display().to_string()),
        ("FUNGI_LAB_DIR", lab.root.display().to_string()),
        ("FUNGI_A_DIR", lab.dir(Target::A).display().to_string()),
        ("FUNGI_B_DIR", lab.dir(Target::B).display().to_string()),
        ("FUNGI_A_PEER_ID", lab.state.node_a.peer_id.clone()),
        ("FUNGI_B_PEER_ID", lab.state.node_b.peer_id.clone()),
        ("FUNGI_RELAY_TCP_ADDR", lab.relay_addresses()[0].clone()),
        ("FUNGI_RELAY_UDP_ADDR", lab.relay_addresses()[1].clone()),
    ] {
        println!("export {name}={}", quote(value));
    }
    Ok(())
}
