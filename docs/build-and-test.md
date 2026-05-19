# Build And Test Guide

Mythenheim copies Fluxheim's habit of making checks executable and repeatable.

## Local Checks

```sh
scripts/checks.sh
```

The first gate runs:

- `cargo fmt --all --check`
- release metadata validation
- Markdown link validation
- migration validation
- `cargo clippy --all-targets -- -D warnings`
- `cargo test`
- reduced feature build checks

Authentication tests cover password policy, Argon2id hashing/verification,
opaque session-token generation, secure cookie handling, and preview auth route
behavior, including malformed JSON, oversized bodies, logout revocation, and
login lockout. Unit tests also ensure the auth store initializes a dummy
password hash for unknown-login timing equalization.

## Smoke Test

```sh
scripts/smoke_local.sh
```

The local smoke validates the example config through the compiled CLI path,
starts the HTTP server on a local port, calls `/healthz`, and exercises the
preview register/login/current-user/logout flow plus category/topic/reply
creation.

## SurrealDB Migration Smoke

```sh
scripts/smoke_surrealdb_migrations.sh
```

This starts a temporary rootless SurrealDB container, renders the built-in
schema migrations with `mythenheim --print-migrations`, applies them twice, and
checks that all migration markers and core tables exist. This is the primary
`0.11.0` integration smoke.

## Fluxheim Wolfi Proxy Smoke

```sh
scripts/smoke_fluxheim_wolfi.sh
```

This starts Mythenheim from `examples/mythenheim-fluxheim-smoke.toml`, runs
Fluxheim's Wolfi container image, mounts
`examples/fluxheim-wolfi-mythenheim.toml`, and verifies that
`Host: mythenheim.eu` and `Host: dev.mythenheim.eu` both proxy to Mythenheim
`/healthz`.

## Security Tools

Install these before release work:

```sh
cargo install --locked cargo-deny
cargo install --locked cargo-audit
```

Then run:

```sh
cargo deny check
cargo deny --all-features check
cargo audit
```

`cargo deny --all-features check` is required before admitting optional
dependency features. It caught the current Rust SDK dependency graph pulling a
known vulnerable RSA crate, so the SDK remains excluded for now.

CI runs these after the project check script.
