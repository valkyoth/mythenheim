#!/usr/bin/env sh
set -eu

cargo fmt --all --check
scripts/validate-release-metadata.sh
perl scripts/check-doc-links.pl
cargo run --quiet -- --check-migrations >/dev/null
cargo clippy --all-targets -- -D warnings
cargo test
cargo check --no-default-features --features web
cargo check --no-default-features
