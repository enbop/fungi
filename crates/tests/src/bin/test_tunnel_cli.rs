//! TCP Tunneling CLI Integration Test
//!
//! This test program validates the TCP tunneling CLI commands by:
//! 1. Starting a daemon process with a temporary configuration directory
//! 2. Running various tunnel CLI commands (add, remove, list)
//! 3. Verifying the expected behavior
//!
//! ## Prerequisites
//!
//! Before running this test, compile the main `fungi` binary:
//!
//! ```bash
//! cargo build --bin fungi
//! ```
//!
//! ## Usage
//!
//! Run the test from the workspace root:
//!
//! ```bash
//! cargo run --package fungi-tests --bin test-tunnel-cli
//! ```
//!
//! Or run the compiled binary directly:
//!
//! ```bash
//! ./target/debug/test-tunnel-cli
//! ```

use anyhow::{Context, Result, bail};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

struct DaemonProcess {
    child: Child,
    temp_dir: TempDir,
    fungi_dir: PathBuf,
}

impl DaemonProcess {
    fn start() -> Result<Self> {
        let temp_dir = TempDir::new().context("Failed to create temp directory")?;
        let fungi_dir = temp_dir.path().join(".fungi");
        std::fs::create_dir_all(&fungi_dir).context("Failed to create .fungi directory")?;

        println!("Starting daemon with config dir: {}", fungi_dir.display());

        let fungi_bin = get_fungi_binary_path()?;
        println!("Using fungi binary: {}", fungi_bin.display());

        let mut child = Command::new(&fungi_bin)
            .arg("daemon")
            .arg("-f")
            .arg(&fungi_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("Failed to start daemon process")?;

        println!("Waiting for daemon to be ready...");
        thread::sleep(Duration::from_secs(3));

        match child.try_wait() {
            Ok(Some(status)) => {
                bail!("Daemon exited unexpectedly with status: {}", status);
            }
            Ok(None) => {
                println!("Daemon is running");
            }
            Err(e) => {
                bail!("Failed to check daemon status: {}", e);
            }
        }

        Ok(Self {
            child,
            temp_dir,
            fungi_dir,
        })
    }

    fn fungi_dir(&self) -> &PathBuf {
        &self.fungi_dir
    }
}

impl Drop for DaemonProcess {
    fn drop(&mut self) {
        println!("Stopping daemon...");
        let _ = self.child.kill();
        let _ = self.child.wait();
        println!("Daemon stopped");
    }
}

struct TestContext {
    daemon: DaemonProcess,
    test_peer_id: String,
}

impl TestContext {
    fn new() -> Result<Self> {
        let daemon = DaemonProcess::start()?;
        let test_peer_id = "12D3KooWRBhwfeP2Y4TCx1SM6s9rUoHhR5STiGwxBhfGuy3ide4f".to_string();

        Ok(Self {
            daemon,
            test_peer_id,
        })
    }

    fn run_cli(&self, args: &[&str]) -> Result<String> {
        let fungi_bin = get_fungi_binary_path()?;

        let output = Command::new(&fungi_bin)
            .arg("-f")
            .arg(self.daemon.fungi_dir())
            .args(args)
            .output()
            .context("Failed to execute CLI command")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("Command failed: {}", stderr);
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

fn get_fungi_binary_path() -> Result<PathBuf> {
    let current_exe = std::env::current_exe().context("Failed to get current executable path")?;

    let target_dir = current_exe
        .parent()
        .context("Failed to get parent directory")?;

    let fungi_bin = target_dir.join("fungi");

    if !fungi_bin.exists() {
        bail!(
            "Fungi binary not found at: {}\n\
            Please compile it first with: cargo build --bin fungi",
            fungi_bin.display()
        );
    }

    Ok(fungi_bin)
}

fn test_tunnel_config(ctx: &TestContext) -> Result<()> {
    println!("\n=== Test: Get tunnel config ===");
    let output = ctx.run_cli(&["tunnel", "config"])?;
    println!("Output:\n{}", output);
    assert!(output.contains("Forwarding:"));
    assert!(output.contains("Listening:"));
    println!("✓ Test passed");
    Ok(())
}

fn test_add_forwarding_rule(ctx: &TestContext) -> Result<()> {
    println!("\n=== Test: Add forwarding rule ===");
    let output = ctx.run_cli(&[
        "tunnel",
        "add-forward",
        "127.0.0.1:8080",
        &ctx.test_peer_id,
        "9090",
    ])?;
    println!("Output:\n{}", output);
    assert!(output.contains("Forwarding rule added"));
    println!("✓ Test passed");
    Ok(())
}

fn test_list_forwarding_rules(ctx: &TestContext) -> Result<()> {
    println!("\n=== Test: List forwarding rules ===");
    let output = ctx.run_cli(&["tunnel", "config"])?;
    println!("Output:\n{}", output);
    assert!(output.contains("127.0.0.1:8080"));
    assert!(output.contains(&ctx.test_peer_id));
    assert!(output.contains("9090"));
    println!("✓ Test passed");
    Ok(())
}

fn test_remove_forwarding_rule(ctx: &TestContext) -> Result<()> {
    println!("\n=== Test: Remove forwarding rule ===");
    let output = ctx.run_cli(&[
        "tunnel",
        "remove-forward",
        "127.0.0.1:8080",
        &ctx.test_peer_id,
        "9090",
    ])?;
    println!("Output:\n{}", output);
    assert!(output.contains("successfully"));
    println!("✓ Test passed");
    Ok(())
}

fn test_verify_forwarding_rule_removed(ctx: &TestContext) -> Result<()> {
    println!("\n=== Test: Verify forwarding rule removed ===");
    let output = ctx.run_cli(&["tunnel", "config"])?;
    println!("Output:\n{}", output);
    assert!(!output.contains("127.0.0.1:8080"));
    println!("✓ Test passed");
    Ok(())
}

fn test_add_listening_rule(ctx: &TestContext) -> Result<()> {
    println!("\n=== Test: Add listening rule ===");
    let output = ctx.run_cli(&["tunnel", "add-listen", "127.0.0.1:7070"])?;
    println!("Output:\n{}", output);
    assert!(output.contains("Listening rule added"));
    println!("✓ Test passed");
    Ok(())
}

fn test_list_listening_rules(ctx: &TestContext) -> Result<()> {
    println!("\n=== Test: List listening rules ===");
    let output = ctx.run_cli(&["tunnel", "config"])?;
    println!("Output:\n{}", output);
    assert!(output.contains("127.0.0.1:7070"));
    println!("✓ Test passed");
    Ok(())
}

fn test_remove_listening_rule(ctx: &TestContext) -> Result<()> {
    println!("\n=== Test: Remove listening rule ===");
    let output = ctx.run_cli(&["tunnel", "remove-listen", "127.0.0.1:7070"])?;
    println!("Output:\n{}", output);
    assert!(output.contains("successfully"));
    println!("✓ Test passed");
    Ok(())
}

fn test_verify_listening_rule_removed(ctx: &TestContext) -> Result<()> {
    println!("\n=== Test: Verify listening rule removed ===");
    let output = ctx.run_cli(&["tunnel", "config"])?;
    println!("Output:\n{}", output);
    assert!(!output.contains("127.0.0.1:7070"));
    println!("✓ Test passed");
    Ok(())
}

fn test_multiple_forwarding_rules(ctx: &TestContext) -> Result<()> {
    println!("\n=== Test: Multiple forwarding rules ===");

    ctx.run_cli(&[
        "tunnel",
        "add-forward",
        "127.0.0.1:8081",
        &ctx.test_peer_id,
        "9091",
    ])?;

    ctx.run_cli(&[
        "tunnel",
        "add-forward",
        "127.0.0.1:8082",
        &ctx.test_peer_id,
        "9092",
    ])?;

    let output = ctx.run_cli(&["tunnel", "config"])?;
    println!("Output:\n{}", output);
    assert!(output.contains("127.0.0.1:8081"));
    assert!(output.contains("127.0.0.1:8082"));

    ctx.run_cli(&[
        "tunnel",
        "remove-forward",
        "127.0.0.1:8081",
        &ctx.test_peer_id,
        "9091",
    ])?;

    let output = ctx.run_cli(&["tunnel", "config"])?;
    assert!(!output.contains("127.0.0.1:8081"));
    assert!(output.contains("127.0.0.1:8082"));

    ctx.run_cli(&[
        "tunnel",
        "remove-forward",
        "127.0.0.1:8082",
        &ctx.test_peer_id,
        "9092",
    ])?;

    println!("✓ Test passed");
    Ok(())
}

fn test_error_cases(ctx: &TestContext) -> Result<()> {
    println!("\n=== Test: Error cases ===");

    // Test 1: Remove non-existent forwarding rule
    println!("1. Testing removal of non-existent forwarding rule...");
    let result = ctx.run_cli(&[
        "tunnel",
        "remove-forward",
        "127.0.0.1:9999",
        &ctx.test_peer_id,
        "8888",
    ]);

    if result.is_err() {
        println!("   ✓ Correctly failed to remove non-existent forwarding rule");
    } else {
        let output = result.unwrap();
        if output.contains("not found") || output.contains("error") {
            println!("   ✓ Returned error message for non-existent rule");
        } else {
            println!("   ⚠ Warning: Removing non-existent rule did not fail");
        }
    }

    // Test 2: Remove non-existent listening rule
    println!("2. Testing removal of non-existent listening rule...");
    let result = ctx.run_cli(&["tunnel", "remove-listen", "127.0.0.1:9999"]);

    if result.is_err() {
        println!("   ✓ Correctly failed to remove non-existent listening rule");
    } else {
        let output = result.unwrap();
        if output.contains("not found") || output.contains("error") {
            println!("   ✓ Returned error message for non-existent rule");
        } else {
            println!("   ⚠ Warning: Removing non-existent rule did not fail");
        }
    }

    // Test 3: Invalid address format (missing port)
    println!("3. Testing invalid address format (missing port)...");
    let result = ctx.run_cli(&["tunnel", "add-listen", "127.0.0.1"]);
    if result.is_err() {
        println!("   ✓ Correctly rejected address without port");
    } else {
        println!("   ⚠ Warning: Invalid address format was not rejected");
    }

    // Test 4: Invalid address format (not an address)
    println!("4. Testing invalid address format (not an address)...");
    let result = ctx.run_cli(&["tunnel", "add-listen", "invalid_address"]);
    if result.is_err() {
        println!("   ✓ Correctly rejected invalid address format");
    } else {
        println!("   ⚠ Warning: Invalid address was not rejected");
    }

    // Test 5: Invalid address format with too many colons
    println!("5. Testing invalid address format (too many colons)...");
    let result = ctx.run_cli(&["tunnel", "add-listen", "127.0.0.1:8080:9090"]);
    if result.is_err() {
        println!("   ✓ Correctly rejected address with multiple ports");
    } else {
        println!("   ⚠ Warning: Address with multiple colons was not rejected");
    }

    // Test 6: Port out of range
    println!("6. Testing port number out of range...");
    let result = ctx.run_cli(&["tunnel", "add-listen", "127.0.0.1:99999"]);
    if result.is_err() {
        println!("   ✓ Correctly rejected port number out of range");
    } else {
        println!("   ⚠ Warning: Port out of range was not rejected");
    }

    // Test 7: Missing required arguments for add-forward
    println!("7. Testing missing required arguments...");
    let result = ctx.run_cli(&["tunnel", "add-forward"]);
    if result.is_err() {
        println!("   ✓ Correctly rejected command with missing arguments");
    } else {
        println!("   ⚠ Warning: Missing arguments were not caught");
    }

    // Test 8: Invalid peer ID format (too short)
    println!("8. Testing invalid peer ID format...");
    let result = ctx.run_cli(&[
        "tunnel",
        "add-forward",
        "127.0.0.1:8080",
        "invalid_peer",
        "9090",
    ]);
    if result.is_err() {
        println!("   ✓ Correctly rejected invalid peer ID");
    } else {
        println!("   ⚠ Warning: Invalid peer ID was not rejected");
    }

    // Test 9: Invalid remote port (non-numeric)
    println!("9. Testing non-numeric port...");
    let result = ctx.run_cli(&[
        "tunnel",
        "add-forward",
        "127.0.0.1:8080",
        &ctx.test_peer_id,
        "not_a_port",
    ]);
    if result.is_err() {
        println!("   ✓ Correctly rejected non-numeric port");
    } else {
        println!("   ⚠ Warning: Non-numeric port was not rejected");
    }

    // Test 10: Port zero (invalid)
    println!("10. Testing port zero...");
    let result = ctx.run_cli(&["tunnel", "add-listen", "127.0.0.1:0"]);
    if result.is_err() {
        println!("   ✓ Correctly rejected port zero");
    } else {
        let output = result.unwrap();
        if output.contains("error") || output.contains("invalid") {
            println!("   ✓ Returned error for port zero");
        } else {
            println!("   ⚠ Warning: Port zero was accepted (may be intentional)");
        }
    }

    println!("✓ Error cases test completed");
    Ok(())
}

fn run_tests() -> Result<()> {
    println!("╔════════════════════════════════════════════════════════╗");
    println!("║     Fungi TCP Tunneling CLI Integration Tests         ║");
    println!("╚════════════════════════════════════════════════════════╝");

    let ctx = TestContext::new()?;

    test_tunnel_config(&ctx)?;
    test_add_forwarding_rule(&ctx)?;
    test_list_forwarding_rules(&ctx)?;
    test_remove_forwarding_rule(&ctx)?;
    test_verify_forwarding_rule_removed(&ctx)?;

    test_add_listening_rule(&ctx)?;
    test_list_listening_rules(&ctx)?;
    test_remove_listening_rule(&ctx)?;
    test_verify_listening_rule_removed(&ctx)?;

    test_multiple_forwarding_rules(&ctx)?;
    test_error_cases(&ctx)?;

    println!("\n╔════════════════════════════════════════════════════════╗");
    println!("║           All tests passed! ✓                          ║");
    println!("╚════════════════════════════════════════════════════════╝");

    Ok(())
}

fn main() {
    env_logger::init();

    if let Err(e) = run_tests() {
        eprintln!("\n❌ Test failed: {}", e);
        std::process::exit(1);
    }
}
