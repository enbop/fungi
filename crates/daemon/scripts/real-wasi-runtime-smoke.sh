#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../../.." && pwd)"
TMP_ROOT="$(mktemp -d /tmp/fungi-wasi-smoke.XXXXXX)"
DATA_DIR="$TMP_ROOT/data"
PROVIDER_PORT="${FUNGI_WASI_PROVIDER_PORT:-18081}"
NAME="${FUNGI_WASI_SMOKE_NAME:-fungi-wasi-smoke}"
WASM_URL="${FUNGI_WASI_WASM_URL:-https://github.com/enbop/socks5-wasip2/releases/download/v0.1.1/socks5-wasip2.wasm}"
WAIT_SECS="${FUNGI_WASI_WAIT_SECS:-10}"

cleanup() {
  rm -rf "$TMP_ROOT"
}
trap cleanup EXIT

cd "$ROOT_DIR"

mkdir -p "$DATA_DIR"

echo "== building binaries =="
cargo build -q -p fungi -p fungi-daemon
echo

echo "== runtime provider smoke =="
cargo run -q -p fungi-daemon --bin test_wasi_runtime -- \
  --launcher "$ROOT_DIR/target/debug/fungi" \
  --wasm-url "$WASM_URL" \
  --name "$NAME" \
  --mount-dir "$DATA_DIR" \
  --mount-target data \
  --port "$PROVIDER_PORT" \
  --wait-secs "$WAIT_SECS" \
  -- \
  --listen "127.0.0.1:$PROVIDER_PORT"
