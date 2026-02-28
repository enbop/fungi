# Project Guidelines

## Code Style

- Keep CLI command modules thin and orchestration-focused; follow the split under [fungi/src/commands](fungi/src/commands) and return `anyhow::Result` in command handlers.
- Put service/business logic in daemon and swarm crates, not CLI dispatch points ([fungi/src/main.rs](fungi/src/main.rs), [crates/daemon/src/api.rs](crates/daemon/src/api.rs)).
- Follow existing shared-state patterns: `Arc<parking_lot::Mutex<_>>` / `Arc<RwLock<_>>` for daemon/swarm control data ([crates/daemon/src/daemon.rs](crates/daemon/src/daemon.rs), [crates/swarm/src/connection_state.rs](crates/swarm/src/connection_state.rs)).
- At RPC boundaries, convert parse/validation failures to `tonic::Status::invalid_argument` and runtime failures to `internal` ([crates/daemon-grpc/src/lib.rs](crates/daemon-grpc/src/lib.rs)).

## Architecture

- Runtime path is CLI -> gRPC client/server -> daemon API -> swarm control/state.
- CLI entry and routing live in [fungi/src/main.rs](fungi/src/main.rs) and [fungi/src/commands/mod.rs](fungi/src/commands/mod.rs).
- gRPC contract source of truth is [crates/daemon-grpc/proto/fungi_daemon.proto](crates/daemon-grpc/proto/fungi_daemon.proto); service implementation is [crates/daemon-grpc/src/lib.rs](crates/daemon-grpc/src/lib.rs).
- Daemon lifecycle/bootstrap is in [crates/daemon/src/daemon.rs](crates/daemon/src/daemon.rs); libp2p runtime is in [crates/swarm/src/libp2p.rs](crates/swarm/src/libp2p.rs).

## Build and Test

- Install protobuf compiler before building (`brew install protobuf` on macOS) per [README.md](README.md).
- Build CLI binary: `cargo build --release --bin fungi`.
- Run daemon locally: `RUST_LOG=info,fungi_swarm=debug cargo run -- daemon`.
- Run command against daemon: `cargo run -- ping <peer_id>`.
- Fast boundary checks after API/proto edits: `cargo check -p fungi-daemon-grpc -p fungi`.
- Integration test crate uses runnable binaries; see [crates/tests/README.md](crates/tests/README.md), e.g. `cargo run --package fungi-tests --bin test-tunnel-cli`.

## Project Conventions

- Treat `SwarmControl::invoke_swarm(...)` as the mutation boundary for libp2p `Swarm`; avoid ad-hoc cross-thread swarm access ([crates/swarm/src/libp2p.rs](crates/swarm/src/libp2p.rs)).
- Do not hand-edit generated gRPC Rust code in [crates/daemon-grpc/src/generated](crates/daemon-grpc/src/generated); edit proto + implementation and rely on [crates/daemon-grpc/build.rs](crates/daemon-grpc/build.rs).
- Note platform behavior in [crates/daemon-grpc/build.rs](crates/daemon-grpc/build.rs): proto generation is skipped for `aarch64-unknown-linux-gnu` and checked-in generated code is used.
- Keep clap aliases/subcommands consistent with existing control command structure ([fungi/src/commands/fungi_control](fungi/src/commands/fungi_control)); docs may contain stale command spelling, so verify against CLI definitions.

## Integration Points

- `fungi-app` depends on the same proto and artifacts; when proto changes, update app stubs via command in [../fungi-app/README.md](../fungi-app/README.md).
- External client behavior is documented in [../fungi-site/docs/grpc-guide.md](../fungi-site/docs/grpc-guide.md); keep RPC field/message compatibility in mind.
- For command UX changes, align CLI behavior with docs in [../fungi-site/docs/cli-service-quick-start.md](../fungi-site/docs/cli-service-quick-start.md).

## Security

- Peer identity keys are local file-based (`.keys/keypair`); key handling is in [crates/util/src/keypair.rs](crates/util/src/keypair.rs) and init path in [crates/config/src/init.rs](crates/config/src/init.rs).
- Access control is PeerID allowlist-driven (`incoming_allowed_peers`) via [crates/config/src/libp2p.rs](crates/config/src/libp2p.rs); preserve this flow when adding inbound features.
- gRPC defaults to loopback `127.0.0.1:5405` ([crates/config/src/rpc.rs](crates/config/src/rpc.rs)); changing bind/address assumptions is security-sensitive.
