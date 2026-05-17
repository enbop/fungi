# Fungi Integration Tests

This crate contains integration tests for the Fungi project.

## Structure

- `src/bin/` - Integration test binaries that can be run standalone

## Available Tests

### WASM Service + Remote CLI Smoke

Starts two temporary daemon nodes, uses one as a WASI service provider, and validates:

- local `service` pull/list/inspect/start/logs/stop/remove`
- `peer capability`, `service pull/list/start/discover/forward/forwarded/unforward/stop/remove`
- HTTP reachability for both direct local service access and remote-forwarded access

**Run:**
```bash
cargo run --package fungi-tests --bin test-service-remote-wasm-cli
```

**Optional environment variables:**
```bash
FUNGI_WASM_URL=https://github.com/enbop/filebrowser-lite/releases/latest/download/filebrowser-lite-wasi.wasm \
FUNGI_WASM_EXPECT_TEXT=filebrowser \
cargo run --package fungi-tests --bin test-service-remote-wasm-cli
```

**Prerequisites:**
- Ensure the `fungi` binary is compiled first:
  ```bash
  cargo build --bin fungi
  ```
- Network access to download the wasm asset, unless `FUNGI_WASM_URL` points to a local/internal URL

## Adding New Tests

1. Create a new binary in `src/bin/`
2. Add it to `Cargo.toml`:
   ```toml
   [[bin]]
   name = "your-test-name"
   path = "src/bin/your_test_name.rs"
   ```
3. Document the test in this README

## Notes

- Tests in this crate are designed to be run independently
- Each test manages its own daemon instances with temporary configurations
- Tests do not interfere with your actual Fungi configuration
