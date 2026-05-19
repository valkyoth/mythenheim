#!/usr/bin/env sh
set -eu

version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
case "$version" in
    0.10.0) ;;
    *)
        echo "expected first commit version 0.10.0, got $version" >&2
        exit 1
        ;;
esac

grep -q '^license = "EUPL-1.2"$' Cargo.toml
grep -q '^rust-version = "1.95"$' Cargo.toml
test -f LICENSE
test -f NOTICE
test -f SECURITY.md
test -f docs/version-plan.md
