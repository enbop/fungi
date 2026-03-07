#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../../.." && pwd)"
TMP_ROOT="$(mktemp -d /tmp/fungi-wasi-smoke.XXXXXX)"
DATA_DIR="$TMP_ROOT/data"
WASM_FILE="$TMP_ROOT/spore-box.wasm"
PROVIDER_PORT="${FUNGI_WASI_PROVIDER_PORT:-18081}"
DIRECT_PORT="${FUNGI_WASI_DIRECT_PORT:-18082}"
NAME="${FUNGI_WASI_SMOKE_NAME:-fungi-wasi-smoke}"
WASM_URL="${FUNGI_WASI_WASM_URL:-https://github.com/enbop/spore-box/releases/download/v0.2.0/spore-box.wasm}"
DIRECT_PID=""

cleanup() {
  if [[ -n "$DIRECT_PID" ]]; then
    kill "$DIRECT_PID" >/dev/null 2>&1 || true
    wait "$DIRECT_PID" >/dev/null 2>&1 || true
  fi
  rm -rf "$TMP_ROOT"
}
trap cleanup EXIT

cd "$ROOT_DIR"

mkdir -p "$DATA_DIR"
curl -L "$WASM_URL" -o "$WASM_FILE"

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
  --wait-secs 3
echo

echo "== direct fungi serve compare =="
(
  cd "$TMP_ROOT"
  "$ROOT_DIR/target/debug/fungi" serve --addr="127.0.0.1:$DIRECT_PORT" -Scli --dir data "$WASM_FILE"
) >"$TMP_ROOT/direct-serve.log" 2>&1 &
DIRECT_PID="$!"
sleep 2

echo "== curl direct fungi serve =="
curl -fsS "http://127.0.0.1:$DIRECT_PORT/?device=smoke"
echo
echo

echo "== direct fungi serve logs =="
cat "$TMP_ROOT/direct-serve.log"