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

Authentication tests cover password policy, Argon2id hashing/verification, and
opaque session-token generation.

## Smoke Test

```sh
scripts/smoke_local.sh
```

The current smoke validates the example config through the compiled CLI path.
Future versions will start the HTTP server, call `/healthz`, and run SurrealDB
integration flows.

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
cargo audit
```

CI runs these after the project check script.
