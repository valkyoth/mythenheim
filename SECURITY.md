# Security Policy

Mythenheim treats security as a release requirement, not a later hardening pass.

## Supported Versions

No stable version exists yet. The project starts at `0.10.0`; all `0.x`
releases are incubator releases. The first planned stable line is `1.0.x`.

| Version | Supported |
| --- | --- |
| `0.x` | Best-effort security fixes before stable |
| `1.0.x` | Planned stable support |

## Reporting

Until a public security contact is published, report issues privately to the
repository owner. Do not publish exploit details before a fix or mitigation is
available.

When reporting, include:

- affected version or commit;
- deployment mode, for example direct binary, rootless Podman, or Fluxheim
  proxy;
- whether SurrealDB, plugins, themes, import/export, or observability are
  involved;
- minimal reproduction steps;
- impact assessment;
- whether you believe the issue is already being exploited.

Do not include real session tokens, passwords, private keys, passkey material,
database credentials, private messages, post contents, emails, or user exports.

## Baseline Rules

- No `unsafe` Rust in core without a documented security review.
- All user-authored content is parsed server-side and sanitized before HTML is
  stored or rendered.
- Session tokens must be opaque, randomly generated, revocable, and stored in
  HttpOnly cookies.
- Privilege checks must use granular capabilities and contextual ownership
  checks, not broad booleans such as `is_admin`.
- WebAssembly plugins must run without filesystem, network, database, or clock
  access unless the host grants a narrow capability.
- Themes receive DTO data only; they must not call arbitrary Rust functions or
  database queries.
- Every release gate must include format, clippy, tests, dependency policy, and
  advisory checks.

## High-Risk Areas

- Authentication, sessions, cookies, CSRF, and password hashing.
- User content parsing, Markdown/BBCode rendering, sanitization, and CSP.
- RBAC, ABAC, trust levels, role assignment, and category permissions.
- Moderation visibility, warnings, bans, shadowbans, queues, and audit logs.
- SurrealDB schema, permissions, migrations, and query parameterization.
- WebAssembly plugin host grants and runtime limits.
- Theme/template rendering, template modifications, and cached templates.
- Imports, exports, backups, and user-owned data packages.
- Logs, metrics, traces, and privacy-sensitive observability.
- Fluxheim forwarded-header trust and public-origin handling.

## Dependency And Supply Chain Policy

- Use crates.io releases by default.
- Avoid git dependencies unless documented.
- Keep `Cargo.lock` committed.
- Run `cargo deny check` and `cargo audit`.
- Review licenses before adding dependencies.
- Treat parser, sanitizer, crypto, auth, template, WASM, and database crates as
  security-sensitive.

## Disclosure Handling

Security fixes should include regression tests when practical. Public release
notes should describe impact and mitigation without publishing exploit details
before users have had a reasonable chance to update.
