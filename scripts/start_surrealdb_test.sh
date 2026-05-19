#!/usr/bin/env sh
set -eu

IMAGE="${MYTHENHEIM_SURREALDB_IMAGE:-docker.io/surrealdb/surrealdb:latest}"
NAME="${MYTHENHEIM_SURREALDB_NAME:-mythenheim-surrealdb-test-$$}"
USER_NAME="${SURREAL_USER:-root}"
PASS="${SURREAL_PASS:-root}"

if [ -z "${CONTAINER_HOST:-}" ] && [ -n "${XDG_RUNTIME_DIR:-}" ] && [ -S "$XDG_RUNTIME_DIR/podman/podman.sock" ]; then
    CONTAINER_HOST="unix://$XDG_RUNTIME_DIR/podman/podman.sock"
    export CONTAINER_HOST
fi

podman run \
    --detach \
    --rm \
    --name "$NAME" \
    --publish 127.0.0.1::8000 \
    "$IMAGE" \
    start \
    --user "$USER_NAME" \
    --pass "$PASS" \
    memory >/dev/null

cleanup() {
    podman rm -f "$NAME" >/dev/null 2>&1 || true
}
trap cleanup INT TERM

port=""
attempt=0
while [ "$attempt" -lt 60 ]; do
    port="$(podman port "$NAME" 8000/tcp | sed -n 's/.*:\([0-9][0-9]*\)$/\1/p' | head -1)"
    if [ -n "$port" ]; then
        break
    fi
    attempt=$((attempt + 1))
    sleep 1
done

if [ -z "$port" ]; then
    echo "failed to discover SurrealDB random host port" >&2
    cleanup
    exit 1
fi

echo "MYTHENHEIM_SURREALDB_CONTAINER=$NAME"
echo "MYTHENHEIM_DATABASE_ENDPOINT=ws://127.0.0.1:$port"
echo "Stop with: podman rm -f $NAME"
