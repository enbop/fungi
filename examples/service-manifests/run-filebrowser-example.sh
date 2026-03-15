#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
EXAMPLE_DIR="$(cd "$(dirname "$0")" && pwd)"
TMP_ROOT="$(mktemp -d /tmp/fungi-filebrowser-example.XXXXXX)"
FUNGI_DIR="$TMP_ROOT/fungi-home"
RPC_ADDR="127.0.0.1:55405"
SERVICE_PORT="${SERVICE_PORT:-28080}"
SERVICE_NAME="filebrowser-example-$$"
MANIFEST_PATH="$TMP_ROOT/${SERVICE_NAME}.service.yaml"
BIN="$ROOT_DIR/target/debug/fungi"
DAEMON_PID=""

cleanup() {
  if [[ -n "$DAEMON_PID" ]]; then
    kill "$DAEMON_PID" >/dev/null 2>&1 || true
    wait "$DAEMON_PID" >/dev/null 2>&1 || true
  fi
  docker rm -f "$SERVICE_NAME" >/dev/null 2>&1 || true
  rm -rf "$TMP_ROOT"
}
trap cleanup EXIT

if ! command -v docker >/dev/null 2>&1; then
  echo "docker is required" >&2
  exit 1
fi

if ! docker info >/dev/null 2>&1; then
  echo "docker daemon is not reachable" >&2
  exit 1
fi

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

[runtime]
disable_docker = false
allowed_host_paths = ["$FUNGI_DIR/services"]
allowed_ports = [$SERVICE_PORT]
allowed_port_ranges = []
EOF

sed \
  -e "s/^  name: filebrowser$/  name: ${SERVICE_NAME}/" \
  -e "s/^    serviceId: filebrowser$/    serviceId: ${SERVICE_NAME}/" \
  -e "s/^    displayName: File Browser$/    displayName: ${SERVICE_NAME}/" \
  "$EXAMPLE_DIR/filebrowser.service.yaml" > "$MANIFEST_PATH"

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

echo "== pull filebrowser manifest =="
"$BIN" --fungi-dir "$FUNGI_DIR" service pull "$MANIFEST_PATH"

echo "== start filebrowser service =="
"$BIN" --fungi-dir "$FUNGI_DIR" service start "$SERVICE_NAME"

echo "== waiting for filebrowser http endpoint =="
ready="false"
for attempt in $(seq 1 40); do
  echo "waiting attempt $attempt/40"
  if curl --connect-timeout 1 --max-time 2 -fsS "http://127.0.0.1:$SERVICE_PORT/" >/dev/null 2>&1; then
    ready="true"
    break
  fi
  sleep 1
done

if [[ "$ready" != "true" ]]; then
  echo "filebrowser endpoint did not become ready" >&2
  echo "== inspect filebrowser service ==" >&2
  "$BIN" --fungi-dir "$FUNGI_DIR" service inspect "$SERVICE_NAME" >&2 || true
  echo "== filebrowser logs ==" >&2
  "$BIN" --fungi-dir "$FUNGI_DIR" service logs "$SERVICE_NAME" --tail 100 >&2 || true
  echo "== daemon log ==" >&2
  tail -n 100 "$TMP_ROOT/daemon.log" >&2 || true
  exit 1
fi

echo "== inspect filebrowser service =="
"$BIN" --fungi-dir "$FUNGI_DIR" service inspect "$SERVICE_NAME"

echo "== curl filebrowser =="
curl -fsS "http://127.0.0.1:$SERVICE_PORT/" | head -n 5
echo

echo "== filebrowser logs =="
"$BIN" --fungi-dir "$FUNGI_DIR" service logs "$SERVICE_NAME" --tail 50

echo
echo "== stopping filebrowser service =="
"$BIN" --fungi-dir "$FUNGI_DIR" service stop "$SERVICE_NAME"

echo "== removing filebrowser service =="
"$BIN" --fungi-dir "$FUNGI_DIR" service remove "$SERVICE_NAME"

echo "== example completed =="
echo "fungi dir: $FUNGI_DIR"
echo "daemon log: $TMP_ROOT/daemon.log"