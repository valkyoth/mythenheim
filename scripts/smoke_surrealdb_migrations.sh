#!/usr/bin/env sh
set -eu

IMAGE="${MYTHENHEIM_SURREALDB_IMAGE:-docker.io/surrealdb/surrealdb:latest}"
NAME="${MYTHENHEIM_SURREALDB_NAME:-mythenheim-surrealdb-migrations-$$}"
NAMESPACE="${MYTHENHEIM_SURREALDB_NAMESPACE:-mythenheim_smoke}"
DATABASE="${MYTHENHEIM_SURREALDB_DATABASE:-mythenheim_smoke}"
USER_NAME="${SURREAL_USER:-root}"
PASS="${SURREAL_PASS:-root}"
MIGRATION_FILE="$(mktemp)"

cleanup() {
    rm -f "$MIGRATION_FILE"
    podman rm -f "$NAME" >/dev/null 2>&1 || true
}
trap cleanup EXIT INT TERM

if [ -z "${CONTAINER_HOST:-}" ] && [ -n "${XDG_RUNTIME_DIR:-}" ] && [ -S "$XDG_RUNTIME_DIR/podman/podman.sock" ]; then
    CONTAINER_HOST="unix://$XDG_RUNTIME_DIR/podman/podman.sock"
    export CONTAINER_HOST
fi

cargo run --quiet -- --check-migrations >/dev/null
cargo run --quiet -- --print-migrations > "$MIGRATION_FILE"

podman run \
    --detach \
    --rm \
    --name "$NAME" \
    --publish 127.0.0.1::8000 \
    "$IMAGE" \
    start \
    --user "$USER_NAME" \
    --pass "$PASS" \
    --bind 0.0.0.0:8000 \
    memory >/dev/null

attempt=0
while [ "$attempt" -lt 60 ]; do
    if printf 'INFO FOR DB;\n' | podman run --rm -i --network "container:$NAME" --entrypoint /surreal "$IMAGE" \
        sql \
        --endpoint ws://127.0.0.1:8000 \
        --username "$USER_NAME" \
        --password "$PASS" \
        --namespace "$NAMESPACE" \
        --database "$DATABASE" \
        --hide-welcome \
        --json >/dev/null 2>&1; then
        break
    fi
    attempt=$((attempt + 1))
    sleep 1
done

if [ "$attempt" -ge 60 ]; then
    echo "SurrealDB did not become ready" >&2
    exit 1
fi

apply_migrations() {
    output="$(podman run --rm -i --network "container:$NAME" --entrypoint /surreal "$IMAGE" \
        sql \
        --endpoint ws://127.0.0.1:8000 \
        --username "$USER_NAME" \
        --password "$PASS" \
        --namespace "$NAMESPACE" \
        --database "$DATABASE" \
        --hide-welcome \
        --pretty < "$MIGRATION_FILE" 2>&1)"

    case "$output" in
        *"There was a problem"*|*"Parse error"*|*"Found field"*|*"Cannot"*|*"Error:"*)
            echo "$output" >&2
            exit 1
            ;;
    esac
}

apply_migrations
apply_migrations

count_json="$(printf 'SELECT count() FROM mythenheim_migration GROUP ALL;\n' | podman run --rm -i --network "container:$NAME" --entrypoint /surreal "$IMAGE" \
    sql \
    --endpoint ws://127.0.0.1:8000 \
    --username "$USER_NAME" \
    --password "$PASS" \
    --namespace "$NAMESPACE" \
    --database "$DATABASE" \
    --hide-welcome \
    --json)"

case "$count_json" in
    *'"count":5'*) ;;
    *)
        echo "expected 5 applied migration markers, got: $count_json" >&2
        exit 1
        ;;
esac

info_json="$(printf 'INFO FOR DB;\n' | podman run --rm -i --network "container:$NAME" --entrypoint /surreal "$IMAGE" \
    sql \
    --endpoint ws://127.0.0.1:8000 \
    --username "$USER_NAME" \
    --password "$PASS" \
    --namespace "$NAMESPACE" \
    --database "$DATABASE" \
    --hide-welcome \
    --json)"

for table in mythenheim_migration user role session category topic post audit_log; do
    case "$info_json" in
        *"\"$table\""*) ;;
        *)
            echo "schema smoke did not find expected table $table: $info_json" >&2
            exit 1
            ;;
    esac
done

echo "surrealdb migrations smoke: ok"
