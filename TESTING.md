# Fungi – Testing Guidelines

This document is the **single source of truth** for testing practices in this codebase.  
Read it before writing a new test or asking an AI agent to write one.

---

## 1. Project-specific pitfalls ("the traps")

These are the places that have caused bugs or confusing test failures before.  
Always test these paths; never skip them just because they look obvious.

### 1.1 Relay config: new positive-model fields

`Network` uses **positive-model** relay flags (`relay_enabled`, `use_community_relays`).  
Older documentation and code used `disable_relay`.  **Never write tests that assert
`!network.disable_relay`** – that field does not exist anymore.

```rust
// ✅ correct
assert!(network.relay_enabled);
assert!(network.use_community_relays);
// ❌ wrong – field removed
assert!(!network.disable_relay);
```

Key invariant to smoke-test: when `relay_enabled = false`, `effective_relay_addresses` must
return an empty vec regardless of `use_community_relays` or `custom_relay_addresses`.

### 1.2 FTP / WebDAV proxy defaults

`FtpProxy::default()` and `WebdavProxy::default()` have `enabled = false` since v0.6.1.  
Tests that rely on the proxy being *on* by default will silently fail to set up tunnels.

### 1.3 Config round-trips

`FungiConfig::save_to_file` + `apply_from_dir` is the canonical persistence path used by
the relay, security, and tunnel commands.  After *any* mutation via the daemon API, verify
the change survives a serialize → deserialize round-trip to the config file.

### 1.4 Port conflicts in daemon tests

Never hard-code TCP ports.  Either use `TestDaemon::spawn()` (which calls `TcpListener::bind
("127.0.0.1:0")` to get an OS-assigned port) or reserve a port the same way before building
a custom config.

### 1.5 `invoke_swarm` is async – don't forget `.await`

`SwarmControl::invoke_swarm` is `async fn`.  Forgetting `.await` silently does nothing
because the returned future is dropped.

### 1.6 `file_transfer` module is deprecated

Do **not** add new tests for the built-in FTP/WebDAV file-transfer functionality.
Existing proxy-default tests have been removed; the module is scheduled for deletion.

### 1.7 `util::ipc` has been deleted

The `fungi_util::ipc` module no longer exists.  Do not import it.  The project uses gRPC
(`fungi-daemon-grpc`) for all inter-process communication.

---

## 2. Test categories and where to put them

| Category | Location | Runner |
|---|---|---|
| Unit tests (pure logic, no I/O) | `#[cfg(test)] mod tests` inside the source file | `cargo test --lib -p <crate>` |
| Integration tests (daemon API, multi-component) | `crates/daemon/tests/` | `cargo test -p fungi-daemon --test <name>` |
| CLI smoke tests (binary + gRPC, real processes) | `crates/tests/src/bin/` | `cargo run --package fungi-tests --bin <name>` |

---

## 3. The `test_support` module

**`fungi_daemon::test_support`** is the canonical helper for daemon-level tests.  
It is always compiled (no feature flag needed) so integration tests can import it directly.

### Quick reference

```rust
use fungi_daemon::test_support::{TestDaemon, TestDaemonBuilder, spawn_connected_pair};

// Single isolated daemon – random key, OS-assigned port, temp dir cleaned up on drop
let d = TestDaemon::spawn().await?;
let pid: PeerId = d.peer_id();
let addr: Multiaddr = d.tcp_multiaddr(); // /ip4/127.0.0.1/tcp/<port>/p2p/<peer>

// Daemon with a known keypair
let kp = Keypair::generate_ed25519();
let d = TestDaemon::spawn_with_keypair(kp).await?;

// Builder: custom allowed peers, known keypair
let server = TestDaemon::spawn().await?;
let client = TestDaemonBuilder::new()
    .with_allowed_peer(server.peer_id())
    .build()
    .await?;

// Pre-wired pair: server allows client, client allows server
let (client, server) = spawn_connected_pair().await?;
// Connect and wait for handshake:
client.connect_to(&server).await?;
client.wait_connected(server.peer_id(), Duration::from_secs(5)).await?;
```

### Design rationale (mirrors `libp2p-swarm-test`)

The `swarm-test` crate in rust-libp2p pioneered the "ephemeral swarm" pattern:

```rust
// libp2p pattern (swarm-test crate)
let mut swarm = Swarm::new_ephemeral_tokio(|_| Behaviour::default());
swarm.wait(|e| match e { SwarmEvent::NewListenAddr { address, .. } => Some(address), _ => None }).await;
```

`TestDaemon` applies the same philosophy to the full `FungiDaemon` stack:
- **Ephemeral identity** – fresh `Keypair::generate_ed25519()` each time.
- **Ephemeral port** – OS selects a free port; no global counters.
- **Ephemeral storage** – `TempDir` auto-deletes on drop.
- **No relay** – tests stay self-contained; no external network dependencies.
- **`wait_connected`** – polls `invoke_swarm(|s| s.is_connected(peer))` with a timeout,
  analogous to libp2p's `SwarmExt::wait`.

---

## 4. Smoke tests (what to test when you don't know where to start)

Smoke tests verify the system boots and basic flows work end-to-end.  
They catch integration bugs that unit tests miss.

### Daemon smoke (unit level)

```rust
#[tokio::test]
async fn daemon_starts_and_has_valid_peer_id() {
    let d = TestDaemon::spawn().await.unwrap();
    assert!(!d.peer_id().to_string().is_empty());
}
```

### Config smoke

After any change to config serialisation, write a round-trip smoke test:

```rust
let original = FungiConfig::default();
let toml = toml::to_string(&original).unwrap();
let parsed: FungiConfig = toml::from_str(&toml).unwrap();
// assert key fields match
```

### CLI smoke (binary level)

See `crates/tests/src/bin/test_relay_config_cli.rs` for the template.  
Pattern: spawn `fungi` binary in a temp dir, run subcommands, assert stdout.

---

## 5. TDD workflow for AI-assisted coding

Use this checklist when asking an AI agent (or following it yourself):

1. **State what the test should prove, not how to implement it.**  
   "After calling `relay-config disable`, `effective_relay_addresses` must return empty."  
   Not: "Add a test that calls the function and checks the result."

2. **Refer to the traps in §1** before generating or reviewing test code.

3. **Use `TestDaemon::spawn()`** as the default.  Only reach for raw `FungiDaemon::start_with`
   when you need to inspect restarted state from a persisted directory.

4. **One assert per test, or give each assert a message.**  
   `assert!(cfg.relay_enabled, "relay should be on by default")`.

5. **Smoke before unit.**  Write a single smoke test first to confirm the component boots.
   Then add focused unit tests for individual behaviors.

6. **Test the error path.**  Config deserialization, invalid peer IDs, port already in use –
   these are the cases that actually break in production.

7. **Don't test deprecated code.**  The `file_transfer` module is deprecated.  The `util::ipc`
   module is deleted.  Tests for them should be removed, not added.

8. **gRPC boundary tests.**  RPC handlers in `crates/daemon-grpc/src/lib.rs` convert domain
   errors to `tonic::Status`.  Add a unit test for any new handler that checks:
   - valid input → returns `Ok`
   - malformed input → returns `Status::invalid_argument`

---

## 6. Running the tests

```bash
# All unit tests
cargo test --lib

# All integration tests for the daemon
cargo test -p fungi-daemon

# Only config tests
cargo test -p fungi-config

# CLI smoke test for relay config (requires built binary)
cargo build --bin fungi
cargo run --package fungi-tests --bin test-relay-config-cli

# Everything
cargo test
```
