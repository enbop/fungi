# webdav-wasip2

Standalone WebDAV extraction based on `crates/daemon/src/controls/file_transfer/webdav_impl.rs`, rewritten to remove fungi-specific RPC/swarm dependencies and to stay within the current `wasm32-wasip2` Tokio support envelope.

## What was extracted

- The `dav-server::fs::DavFileSystem` adapter logic from fungi's `webdav_impl.rs`
- Buffered WebDAV file writes and seek/flush behavior
- WebDAV HTTP serving on top of `hyper` + `tokio::net::TcpListener`

## What changed

- `FileTransferClientsControl` was replaced by a small `WebDavBackend` trait
- fungi metadata/error types were replaced by local `Metadata`, `DirEntry`, and `BackendError`
- the WASI demo uses an in-memory backend instead of `tokio::fs`, because `tokio::fs` is not currently available on `wasm32-wasip2`
- the server uses `#[tokio::main(flavor = "current_thread")]` to match current Tokio WASI support

## Current Tokio `wasm32-wasip2` assumptions

This crate follows the support scope you provided:

- `current_thread`, `tokio::spawn`, `tokio::select!`, timers, and sync primitives are usable
- `TcpListener`/`TcpStream` work with `RUSTFLAGS=--cfg tokio_unstable`
- `tokio::fs`, DNS, process, and multi-thread runtime are intentionally avoided here

## Native smoke test

```bash
cargo run --manifest-path /home/runner/work/fungi/fungi/webdav-wasip2/Cargo.toml -- --smoke-test
```

## Native server

```bash
cargo run --manifest-path /home/runner/work/fungi/fungi/webdav-wasip2/Cargo.toml -- --addr 127.0.0.1:8080
```

Then browse or mount the WebDAV endpoint and try `/hello.txt`.

## `wasm32-wasip2` smoke test

```bash
rustup target add wasm32-wasip2

RUSTFLAGS="--cfg tokio_unstable" CARGO_TARGET_WASM32_WASIP2_RUNNER="wasmtime run -Sinherit-network" cargo run   --manifest-path /home/runner/work/fungi/fungi/webdav-wasip2/Cargo.toml   --target wasm32-wasip2   -- --smoke-test
```

## `wasm32-wasip2` server

```bash
RUSTFLAGS="--cfg tokio_unstable" CARGO_TARGET_WASM32_WASIP2_RUNNER="wasmtime run -Sinherit-network" cargo run   --manifest-path /home/runner/work/fungi/fungi/webdav-wasip2/Cargo.toml   --target wasm32-wasip2   -- --addr 0.0.0.0:8080
```

## Notes

- This is intentionally a standalone crate and is not part of the fungi workspace.
- The demo backend is in-memory so it can run under native and `wasm32-wasip2` without relying on unsupported Tokio filesystem APIs.
