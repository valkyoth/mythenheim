# Contributing To Mythenheim

Mythenheim is a security-sensitive forum platform. Contributions are welcome
when they keep the project clear, tested, documented, and honest about what is
stable.

## License

Mythenheim is licensed under the European Union Public Licence 1.2. By
contributing, you agree that your contribution is provided under the same
license.

User-created WebAssembly plugins and themes are covered by the additional
notice in [NOTICE](../NOTICE).

## Development Setup

Use the pinned Rust toolchain from `rust-toolchain.toml`.

```bash
cargo build
cargo test
```

Useful reduced builds:

```bash
cargo check --no-default-features
cargo check --no-default-features --features web
```

## Checks

Before opening a pull request, run:

```bash
scripts/checks.sh
scripts/smoke_local.sh
```

When changing Fluxheim deployment examples or proxy behavior, also run:

```bash
scripts/smoke_fluxheim_wolfi.sh
```

When changing dependencies, run:

```bash
cargo deny check
cargo audit
```

## Security-Sensitive Changes

Treat these areas as high risk:

- authentication, sessions, cookies, CSRF, and password handling;
- user content parsing, Markdown/BBCode rendering, sanitization, and CSP;
- RBAC, ABAC, trust levels, role assignment, and category permissions;
- moderation, warnings, mutes, bans, shadowban visibility, and audit logs;
- plugin host capabilities, WASM execution, template rendering, and themes;
- SurrealDB schema, permissions, migrations, and query parameterization;
- imports, exports, backups, and user data portability;
- logging, metrics, tracing, and privacy-sensitive observability;
- dependency updates.

Do not post exploitable security details in public issues. Follow
[SECURITY.md](../SECURITY.md).

## Dependency Policy

Mythenheim uses `deny.toml`, `cargo-deny`, and `cargo-audit`.

When adding or updating crates:

- prefer crates.io releases;
- avoid git dependencies unless a design note explains why;
- check maintenance status and license;
- keep `Cargo.lock` updated;
- run `cargo deny check` and `cargo audit`;
- document security-impacting dependencies in the relevant roadmap or design
  doc.

## Design Guidelines

- Prefer existing local patterns over new abstractions.
- Keep feature work mapped to [docs/version-plan.md](../docs/version-plan.md).
- Add tests for behavior changes.
- Add docs or examples when operator-facing behavior changes.
- Keep default builds focused on stable or actively tested core behavior.
- Do not add extension/plugin/theme capabilities without an explicit security
  boundary.
- Keep logs, metrics, and traces privacy-safe and cardinality-safe.

## Pull Requests

Good pull requests are small enough to review and include:

- a clear summary;
- tests for behavior changes;
- docs or examples for user-facing behavior;
- security notes for risky areas;
- follow-up work called out honestly.

Large features should start with a roadmap or design-doc update before code.
