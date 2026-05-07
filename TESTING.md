# Fungi Testing Guide

## Pick the right test

| If you are testing... | Put the test here | Run with |
|---|---|---|
| Pure logic with no I/O | `#[cfg(test)] mod tests` in the same file | `cargo test --lib -p <crate>` |
| Daemon API behavior or multiple components working together | `crates/daemon/tests/` | `cargo test -p fungi-daemon --test <name>` |
| The real CLI talking to real processes over gRPC | `crates/tests/src/bin/` | `cargo run --package fungi-tests --bin <name>` |

Start with the smallest test that proves the behavior you care about. Move to integration or CLI tests only when the behavior crosses process or API boundaries.

## Use `test_support` for daemon tests

`fungi_daemon::test_support` should be the default for tests that need a running `FungiDaemon`. It gives you temp dirs, random ports, and cleanup automatically, so you do not need to hand-roll test setup.

```rust
use fungi_daemon::test_support::{TestDaemon, TestDaemonBuilder, spawn_connected_pair};

// Single isolated daemon
let d = TestDaemon::spawn().await?;
let pid: PeerId   = d.peer_id();
let addr: Multiaddr = d.tcp_multiaddr(); // /ip4/127.0.0.1/tcp/<port>/p2p/<peer>

// Deterministic PeerId
let d = TestDaemon::spawn_with_keypair(Keypair::generate_ed25519()).await?;

// Custom setup
let server = TestDaemon::spawn().await?;
let client = TestDaemonBuilder::new()
    .with_allowed_peer(server.peer_id())
    .build().await?;

// Connected pair
let (client, server) = spawn_connected_pair().await?;
client.connect_to(&server).await?;
client.wait_connected(server.peer_id(), Duration::from_secs(5)).await?;
```

## Running tests

```bash
cargo test --lib                   # all unit tests
cargo test -p fungi-daemon         # daemon unit + integration tests
cargo test                         # everything

# CLI smoke test (requires built binary)
cargo build --bin fungi
cargo run --package fungi-tests --bin test-relay-config-cli
```

## Local CLI lab

Use `fungi-lab` when you want an interactive, real-process A/B environment instead of a one-off test binary. It starts one local relay plus two daemons, disables community relays, saves both devices, and defaults to trusting node A on node B so A can test services on B.

```bash
cargo build -p fungi -p fungi-lab
./target/debug/fungi-lab start

./target/debug/fungi -f target/tmp_a service list
./target/debug/fungi -f target/tmp_b device trusted

./target/debug/fungi-lab node stop b
./target/debug/fungi-lab node start b
./target/debug/fungi-lab stop
```
