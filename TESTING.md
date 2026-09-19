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

`fungi-lab` creates and operates a local relay + A/B daemon environment. Source
checkout, compilation, test scenarios, and assertions belong to the caller.
Reuse existing binaries and the Rust build cache.

```bash
cargo build -p fungi -p fungi-lab
./target/debug/fungi-lab start
eval "$(./target/debug/fungi-lab env)"

"$FUNGI_BIN" -f "$FUNGI_A_DIR" info id
"$FUNGI_BIN" -f "$FUNGI_B_DIR" device trusted
./target/debug/fungi-lab node restart a
./target/debug/fungi-lab relay restart
./target/debug/fungi-lab stop
```

All commands select the same instance with `--lab-dir PATH` or
`FUNGI_LAB_DIR` (CLI > environment > `target/local-lab` in the checkout).
This is a data directory containing `state.json`, logs, `relay-home/`, and
`nodes/{a,b}/fungi/`. Fungi's sibling user directories stay under each node.
`start --fungi-bin PATH` selects another built binary; later commands reuse it.

There is no background manager, watchdog, or automatic expiry. Each command
performs its operation and exits; relay and daemons continue until stopped.
Tests and Agents must call `stop` in their cleanup path. `clean` also deletes
the owned lab directory, so use it only after retaining any needed evidence.
`start` refuses to replace any running lab process. After `stop`, it reuses
identities and resets trust to the requested mode. Restart appends log boundaries;
relay identity and ports remain stable.

Nodes use dynamic RPC/TCP/UDP ports. Use their `fungi-dir` with the Fungi CLI;
it discovers RPC through `daemon.endpoint`. Lab state does not cache these
ports or a readiness/trust flag. `status --json` combines recorded identities
with live process checks and derived paths; running is not proof of readiness or
connectivity. Use `info id`, `device trusted`, and bounded `ping` to test those.

Trust defaults to none. Explicitly use `start --trust b-trusts-a` or
`trust b-trusts-a` for B to grant A service-management access. Other modes are
`a-trusts-b`, `both`, and `none`. Grants print peer IDs, host-path exposure,
and revocation instructions. If a trust operation fails, inspect both nodes:
a partial grant may have persisted. Do not use lab trust commands for real devices.

Mutating commands hold one file lock. State writes are atomic; each spawned
process is recorded before readiness checks. Normal startup failures/timeouts
reclaim only that operation's new children and retain logs. Abruptly killing the
lab command can leave processes running; use `stop` with the same lab directory
to recover recorded children. There is no claim of automatic recovery from
SIGKILL or a crash between spawning a child and recording it.

State version 3 rejects old/invalid state and unrecognized directories without
deleting them. Stop/clean older labs with the previous binary before upgrading,
or select a fresh directory. There is no migration, force-delete, automatic port
retry, or multi-node topology configuration. Only the relay still needs explicit
ports; conflicts fail with logs retained.

```bash
cargo test -p fungi-lab
# Requires built fungi/fungi-lab and local sockets; creates only temporary labs.
cargo test -p fungi-lab real_ -- --ignored --test-threads=1
```

The checks cover state/ownership, scoped rollback and timeout cleanup, dynamic
node configuration, CLI persistence and RPC discovery after restart, trust
directions, and log retention. They do not deploy Docker services.
