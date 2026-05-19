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
- `cargo clippy --all-targets -- -D warnings`
- `cargo test`
- reduced feature build checks

## Smoke Test

```sh
scripts/smoke_local.sh
```

The current smoke validates the example config through the compiled CLI path.
Future versions will start the HTTP server, call `/healthz`, and run SurrealDB
integration flows.

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
