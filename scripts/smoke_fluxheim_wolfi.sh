#!/usr/bin/env sh
set -eu

FLUXHEIM_DIR="${FLUXHEIM_DIR:-/home/eldryoth/Work/codex-projects/fluxheim}"
FLUXHEIM_IMAGE="${FLUXHEIM_IMAGE:-fluxheim:wolfi-mythenheim}"
FLUXHEIM_CONTAINER="${FLUXHEIM_CONTAINER:-mythenheim-fluxheim-wolfi-smoke-$$}"
MYTHENHEIM_CONFIG="${MYTHENHEIM_CONFIG:-examples/mythenheim-fluxheim-smoke.toml}"
FLUXHEIM_CONFIG="${FLUXHEIM_CONFIG:-$PWD/examples/fluxheim-wolfi-mythenheim.toml}"

app_pid=""

cleanup() {
    if [ -n "$app_pid" ]; then
        kill "$app_pid" >/dev/null 2>&1 || true
    fi
    podman rm -f "$FLUXHEIM_CONTAINER" >/dev/null 2>&1 || true
}
trap cleanup EXIT INT TERM

if [ ! -d "$FLUXHEIM_DIR" ]; then
    echo "Fluxheim repo not found: $FLUXHEIM_DIR" >&2
    exit 1
fi

if [ -z "${CONTAINER_HOST:-}" ] && [ -n "${XDG_RUNTIME_DIR:-}" ] && [ -S "$XDG_RUNTIME_DIR/podman/podman.sock" ]; then
    CONTAINER_HOST="unix://$XDG_RUNTIME_DIR/podman/podman.sock"
    export CONTAINER_HOST
fi

if ! podman image exists "$FLUXHEIM_IMAGE"; then
    podman build \
        -t "$FLUXHEIM_IMAGE" \
        -f "$FLUXHEIM_DIR/containers/Containerfile.wolfi" \
        "$FLUXHEIM_DIR"
fi

cargo run --quiet -- --config "$MYTHENHEIM_CONFIG" &
app_pid="$!"

attempt=0
until curl -sSf http://127.0.0.1:37171/healthz >/dev/null; do
    if ! kill -0 "$app_pid" >/dev/null 2>&1; then
        echo "Mythenheim exited before becoming healthy" >&2
        exit 1
    fi
    attempt=$((attempt + 1))
    if [ "$attempt" -gt 60 ]; then
        echo "Mythenheim did not become healthy" >&2
        exit 1
    fi
    sleep 1
done

podman run \
    --detach \
    --rm \
    --name "$FLUXHEIM_CONTAINER" \
    --publish 127.0.0.1::8080 \
    --volume "$FLUXHEIM_CONFIG:/etc/fluxheim/fluxheim.toml:ro,Z" \
    "$FLUXHEIM_IMAGE" \
    --config /etc/fluxheim/fluxheim.toml >/dev/null

port=""
proxied_ok=0
attempt=0
while [ "$attempt" -lt 60 ]; do
    if ! podman inspect "$FLUXHEIM_CONTAINER" >/dev/null 2>&1; then
        echo "Fluxheim container exited before proxy smoke completed" >&2
        podman logs "$FLUXHEIM_CONTAINER" >&2 || true
        exit 1
    fi
    port="$(podman port "$FLUXHEIM_CONTAINER" 8080/tcp | sed -n 's/.*:\([0-9][0-9]*\)$/\1/p' | head -1)"
    if [ -n "$port" ] && curl -sSf -H "Host: mythenheim.eu" "http://127.0.0.1:$port/healthz" >/dev/null; then
        proxied_ok=1
        break
    fi
    attempt=$((attempt + 1))
    sleep 1
done

if [ -z "$port" ]; then
    echo "failed to discover Fluxheim random host port" >&2
    exit 1
fi

if [ "$proxied_ok" -ne 1 ]; then
    echo "Fluxheim did not proxy Mythenheim successfully" >&2
    podman logs "$FLUXHEIM_CONTAINER" >&2 || true
    exit 1
fi

curl -sSf -H "Host: mythenheim.eu" "http://127.0.0.1:$port/healthz" >/dev/null
curl -sSf -H "Host: dev.mythenheim.eu" "http://127.0.0.1:$port/healthz" >/dev/null

echo "fluxheim wolfi smoke: ok on 127.0.0.1:$port"
