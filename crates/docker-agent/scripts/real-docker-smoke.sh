#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../../.." && pwd)"
CRATE_DIR="$ROOT_DIR/crates/docker-agent"

detect_socket() {
  if [[ -n "${DOCKER_HOST:-}" && "${DOCKER_HOST}" == unix://* ]]; then
    printf '%s\n' "${DOCKER_HOST#unix://}"
    return 0
  fi

  if [[ -S "$HOME/.docker/run/docker.sock" ]]; then
    printf '%s\n' "$HOME/.docker/run/docker.sock"
    return 0
  fi

  if [[ -S "/var/run/docker.sock" ]]; then
    printf '%s\n' "/var/run/docker.sock"
    return 0
  fi

  return 1
}

if ! command -v docker >/dev/null 2>&1; then
  echo "docker command not found" >&2
  exit 1
fi

if ! docker info >/dev/null 2>&1; then
  echo "docker daemon is not reachable" >&2
  exit 1
fi

SOCKET_PATH="${DOCKER_SOCKET_PATH:-$(detect_socket)}"
NAME="${FUNGI_SMOKE_NAME:-fungi-smoke-$(date +%s)}"
HOST_PORT="${FUNGI_SMOKE_PORT:-18080}"
ALLOWED_ROOT="$(mktemp -d /tmp/fungi-docker-smoke.XXXXXX)"
MOUNT_HOST="$ALLOWED_ROOT/site"

cleanup() {
  docker rm -f "$NAME" >/dev/null 2>&1 || true
  rm -rf "$ALLOWED_ROOT"
}
trap cleanup EXIT

cd "$ROOT_DIR"

echo "== docker environment =="
echo "socket: $SOCKET_PATH"
echo "container: $NAME"
echo "host port: $HOST_PORT"
echo

docker pull nginx:alpine >/dev/null

run_cli() {
  cargo run -q -p fungi-docker-agent --bin docker-agent-smoke -- \
    --socket "$SOCKET_PATH" \
    --allowed-root "$ALLOWED_ROOT" \
    --mount-host "$MOUNT_HOST" \
    --name "$NAME" \
    --host-port "$HOST_PORT" \
    "$@"
}

echo "== agent create =="
run_cli create
echo

echo "== docker inspect after create =="
docker inspect "$NAME"
echo

echo "== agent start =="
run_cli start
echo

sleep 1

echo "== docker ps =="
docker ps --filter "name=$NAME"
echo

echo "== curl localhost =="
curl -fsS "http://127.0.0.1:$HOST_PORT/"
echo
echo

echo "== agent logs =="
run_cli logs
echo

echo "== docker logs =="
docker logs "$NAME"
echo

echo "== agent stop =="
run_cli stop
echo

echo "== docker inspect state after stop =="
docker inspect --format '{{.State.Status}}' "$NAME"
echo

echo "== agent remove =="
run_cli remove
echo

echo "== docker ps -a after remove =="
docker ps -a --filter "name=$NAME"