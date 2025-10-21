# Fungi Integration Tests

This crate contains integration tests for the Fungi project.

## Structure

- `src/bin/` - Integration test binaries that can be run standalone

## Available Tests

### TCP Tunneling CLI Tests

Tests the TCP tunneling CLI commands end-to-end.

**Run:**
```bash
# From workspace root
cargo run --package fungi-tests --bin test-tunnel-cli

# Or compile and run directly
cargo build --package fungi-tests --bin test-tunnel-cli
./target/debug/test-tunnel-cli
```

**Prerequisites:**
- Ensure the `fungi` binary is compiled first:
  ```bash
  cargo build --bin fungi
  ```

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
