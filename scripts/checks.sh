#!/usr/bin/env sh
set -eu

cargo fmt --all --check
scripts/validate-release-metadata.sh
perl scripts/check-doc-links.pl
python3 -c 'import ast, pathlib; ast.parse(pathlib.Path("scripts/build_release_binary.py").read_text())'
cargo run --quiet -- --check-migrations >/dev/null
cargo clippy --all-targets -- -D warnings
cargo test
cargo check --no-default-features --features web
cargo check --no-default-features
