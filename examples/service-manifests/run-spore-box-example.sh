#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
EXAMPLE_DIR="$(cd "$(dirname "$0")" && pwd)"
TMP_ROOT="$(mktemp -d /tmp/fungi-spore-box-example.XXXXXX)"
FUNGI_DIR="$TMP_ROOT/fungi-home"
RPC_ADDR="127.0.0.1:55406"
SERVICE_PORT="28081"
BIN="$ROOT_DIR/target/debug/fungi"
DAEMON_PID=""

cleanup() {
  if [[ -n "$DAEMON_PID" ]]; then
    kill "$DAEMON_PID" >/dev/null 2>&1 || true
    wait "$DAEMON_PID" >/dev/null 2>&1 || true
  fi
  rm -rf "$TMP_ROOT"
}
trap cleanup EXIT

mkdir -p "$FUNGI_DIR"

if lsof -nP -iTCP:"$SERVICE_PORT" -sTCP:LISTEN >/dev/null 2>&1; then
  echo "service port $SERVICE_PORT is already in use" >&2
  lsof -nP -iTCP:"$SERVICE_PORT" -sTCP:LISTEN >&2 || true
  exit 1
fi

cd "$ROOT_DIR"
echo "== building fungi =="
cargo build -q -p fungi

echo "== initializing fungi dir =="
"$BIN" --fungi-dir "$FUNGI_DIR" init

cat > "$FUNGI_DIR/config.toml" <<EOF
[rpc]
listen_address = "$RPC_ADDR"

[file_transfer.server]
enabled = false
shared_root_dir = ""

[file_transfer.proxy_ftp]
enabled = false
host = "127.0.0.1"
port = 2121

[file_transfer.proxy_webdav]
enabled = false
host = "127.0.0.1"
port = 8181

[docker]
enabled = false
allowed_host_paths = []
allowed_ports = []
EOF

echo "== starting daemon =="
"$BIN" --fungi-dir "$FUNGI_DIR" daemon >"$TMP_ROOT/daemon.log" 2>&1 &
DAEMON_PID="$!"

echo "== waiting for daemon =="
daemon_ready="false"
for attempt in $(seq 1 30); do
  version="$($BIN --fungi-dir "$FUNGI_DIR" info version 2>/dev/null || true)"
  if [[ -n "$version" ]]; then
    daemon_ready="true"
    echo "daemon ready: $version"
    break
  fi
  echo "daemon wait attempt $attempt/30"
  sleep 1
done

if [[ "$daemon_ready" != "true" ]]; then
  echo "daemon did not become ready" >&2
  echo "== daemon log ==" >&2
  tail -n 100 "$TMP_ROOT/daemon.log" >&2 || true
  exit 1
fi

echo "== deploy spore-box manifest =="
"$BIN" --fungi-dir "$FUNGI_DIR" service deploy "$EXAMPLE_DIR/spore-box.service.yaml"

echo "== start spore-box service =="
"$BIN" --fungi-dir "$FUNGI_DIR" service start wasmtime spore-box

echo "== waiting for spore-box http endpoint =="
ready="false"
for attempt in $(seq 1 40); do
  echo "waiting attempt $attempt/40"
  if curl --connect-timeout 1 --max-time 2 -fsS "http://127.0.0.1:$SERVICE_PORT/?device=manifest" >/dev/null 2>&1; then
    ready="true"
    break
  fi
  sleep 1
done

if [[ "$ready" != "true" ]]; then
  echo "spore-box endpoint did not become ready" >&2
  echo "== inspect spore-box service ==" >&2
  "$BIN" --fungi-dir "$FUNGI_DIR" service inspect wasmtime spore-box >&2 || true
  echo "== spore-box logs ==" >&2
  "$BIN" --fungi-dir "$FUNGI_DIR" service logs wasmtime spore-box --tail 100 >&2 || true
  echo "== daemon log ==" >&2
  tail -n 100 "$TMP_ROOT/daemon.log" >&2 || true
  exit 1
fi

echo "== inspect spore-box service =="
"$BIN" --fungi-dir "$FUNGI_DIR" service inspect wasmtime spore-box

echo "== curl spore-box =="
curl -fsS "http://127.0.0.1:$SERVICE_PORT/?device=manifest"
echo

echo "== spore-box logs =="
"$BIN" --fungi-dir "$FUNGI_DIR" service logs wasmtime spore-box --tail 50

echo
echo "== stopping spore-box service =="
"$BIN" --fungi-dir "$FUNGI_DIR" service stop wasmtime spore-box

echo "== removing spore-box service =="
"$BIN" --fungi-dir "$FUNGI_DIR" service remove wasmtime spore-box

echo "== example completed =="
echo "fungi dir: $FUNGI_DIR"
echo "daemon log: $TMP_ROOT/daemon.log"