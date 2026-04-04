# Fungi – Testing Guidelines

---

## Test categories

| Category | Location | Runner |
|---|---|---|
| Unit tests (pure logic, no I/O) | `#[cfg(test)] mod tests` inside the source file | `cargo test --lib -p <crate>` |
| Integration tests (daemon API, multi-component) | `crates/daemon/tests/` | `cargo test -p fungi-daemon --test <name>` |
| CLI smoke tests (binary + gRPC, real processes) | `crates/tests/src/bin/` | `cargo run --package fungi-tests --bin <name>` |

---

## `test_support` — ephemeral daemon helpers

**`fungi_daemon::test_support`** provides RAII test daemons inspired by
[libp2p-swarm-test](https://github.com/libp2p/rust-libp2p/tree/master/swarm-test).
Use it as the default for any test that needs a running `FungiDaemon`.

```rust
use fungi_daemon::test_support::{TestDaemon, TestDaemonBuilder, spawn_connected_pair};

// Single isolated daemon – random identity, OS-assigned port, temp dir auto-deleted
let d = TestDaemon::spawn().await?;
let pid: PeerId   = d.peer_id();
let addr: Multiaddr = d.tcp_multiaddr(); // /ip4/127.0.0.1/tcp/<port>/p2p/<peer>

// Known keypair (deterministic PeerId)
let d = TestDaemon::spawn_with_keypair(Keypair::generate_ed25519()).await?;

// Builder: add allowed peers, custom keypair
let server = TestDaemon::spawn().await?;
let client = TestDaemonBuilder::new()
    .with_allowed_peer(server.peer_id())
    .build().await?;

// Pre-wired pair (each side allows the other)
let (client, server) = spawn_connected_pair().await?;
client.connect_to(&server).await?;
client.wait_connected(server.peer_id(), Duration::from_secs(5)).await?;
```

Key properties: ephemeral identity, OS-reserved port (no hard-coded ports, no counters),
relay disabled, `TempDir` cleaned up on drop.  Only reach for raw `FungiDaemon::start_with`
when a test needs to inspect restarted state from a persisted directory.

---

## What to test (and what not to)

**Test behaviour, not defaults.**  A test that only asserts a struct's default field value
will break whenever that default is intentionally changed — it gives no signal about whether
the feature works.  Write tests that exercise a code path:

```rust
// ❌ fragile — breaks on any intentional default change
assert_eq!(network.idle_connection_timeout_secs, 300);

// ✅ tests the actual behaviour
network.relay_enabled = false;
assert!(network.effective_relay_addresses(&community).is_empty());
```

**Smoke before unit.**  For any new component, start with a single smoke test that confirms
it boots/parses/runs, then add focused tests for non-obvious paths.

**Test the error path.**  Invalid input, missing files, parse failures — these are the cases
that actually break in production and are often skipped.

**Config round-trips.**  After any mutation via the daemon API, verify the change survives
a `save_to_file` → `apply_from_dir` round-trip.

**gRPC boundary.**  RPC handlers convert domain errors to `tonic::Status`.  For each new
handler, add a unit test that checks valid input → `Ok` and malformed input →
`Status::invalid_argument`.

**Don't test deprecated code.**  `file_transfer` is deprecated; `util::ipc` is deleted.
Do not add tests for either.

---

## TDD workflow for AI-assisted coding

1. **State what the test should prove**, not how to implement it.
2. **One assert per test**, or give each assert a descriptive message.
3. **Use `TestDaemon::spawn()`** as the default for daemon-level tests.
4. **`invoke_swarm` is async** — always `.await` it or the future is silently dropped.

---

## Running the tests

```bash
cargo test --lib                   # all unit tests
cargo test -p fungi-daemon         # daemon unit + integration tests
cargo test                         # everything

# CLI smoke test (requires built binary)
cargo build --bin fungi
cargo run --package fungi-tests --bin test-relay-config-cli
```
