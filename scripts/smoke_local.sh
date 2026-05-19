#!/usr/bin/env sh
set -eu

config="${MYTHENHEIM_CONFIG:-examples/mythenheim.toml}"

cargo run --quiet -- --check-config --config "$config" >/dev/null
if [ "$config" = "examples/mythenheim.toml" ]; then
    cargo run --quiet -- --check-config --config examples/mythenheim-dev.toml >/dev/null
fi
echo "smoke local: config ok"
