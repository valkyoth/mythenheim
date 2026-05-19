#!/usr/bin/env sh
set -eu

config="${MYTHENHEIM_CONFIG:-examples/mythenheim.toml}"
port="${MYTHENHEIM_SMOKE_PORT:-$((37000 + ($$ % 10000)))}"
runtime_config="${TMPDIR:-/tmp}/mythenheim-smoke-$$.toml"
cookie_jar="${TMPDIR:-/tmp}/mythenheim-smoke-cookies-$$.txt"
app_pid=""

cleanup() {
    if [ -n "$app_pid" ]; then
        kill "$app_pid" >/dev/null 2>&1 || true
    fi
    rm -f "$runtime_config" "$cookie_jar"
}
trap cleanup EXIT INT TERM

cargo run --quiet -- --check-config --config "$config" >/dev/null
if [ "$config" = "examples/mythenheim.toml" ]; then
    cargo run --quiet -- --check-config --config examples/mythenheim-dev.toml >/dev/null
fi

sed \
    -e "s/listen_addr = \"127\\.0\\.0\\.1:[0-9][0-9]*\"/listen_addr = \"127.0.0.1:$port\"/" \
    -e "s/secure_cookies = true/secure_cookies = false/" \
    "$config" >"$runtime_config"

cargo run --quiet -- --config "$runtime_config" &
app_pid="$!"

attempt=0
until curl -sSf "http://127.0.0.1:$port/healthz" >/dev/null 2>&1; do
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

username="Smoke_$port"
email="smoke-$port@example.test"
password="correct horse battery staple"
register_body="{\"username\":\"$username\",\"email\":\"$email\",\"password\":\"$password\"}"
login_body="{\"login\":\"$username\",\"password\":\"$password\"}"

curl -sSf \
    -X POST "http://127.0.0.1:$port/api/v1/auth/register" \
    -H "content-type: application/json" \
    -d "$register_body" >/dev/null

curl -sSf \
    -X POST "http://127.0.0.1:$port/api/v1/auth/login" \
    -H "content-type: application/json" \
    -c "$cookie_jar" \
    -d "$login_body" >/dev/null

me_body="$(curl -sSf -b "$cookie_jar" "http://127.0.0.1:$port/api/v1/auth/me")"
case "$me_body" in
    *"\"username\":\"$username\""*) ;;
    *)
        echo "current-user response did not contain expected username" >&2
        exit 1
        ;;
esac

curl -sSf \
    -X POST \
    -b "$cookie_jar" \
    -c "$cookie_jar" \
    "http://127.0.0.1:$port/api/v1/auth/logout" >/dev/null

status_after_logout="$(curl -s -o /dev/null -w "%{http_code}" -b "$cookie_jar" "http://127.0.0.1:$port/api/v1/auth/me")"
if [ "$status_after_logout" != "401" ]; then
    echo "expected 401 after logout, got $status_after_logout" >&2
    exit 1
fi

echo "smoke local: config and auth api ok on 127.0.0.1:$port"
