# Architecture Plan

Mythenheim is an API-first forum application with a Rust core, SurrealDB
document/graph storage, and optional server-rendered or external frontends. The
compiled binary is intended to run directly on Linux, macOS, and Windows. Linux
is also the container target for rootless Podman deployments behind Fluxheim.
BSD remains a best-effort source portability goal.

## Core Components

- HTTP: Axum on Tokio.
- Storage: SurrealDB over WebSocket for production and rootless Podman tests.
- Content: Markdown first, BBCode compatibility later, server-side HTML
  sanitization before persistence.
- Auth: opaque cookie sessions, Argon2id password hashes, passkeys in a later
  preview release, revocation support from the first auth milestone.
- Permissions: capability strings plus contextual ABAC checks.
- Observability: OpenTelemetry tracing and metrics with Prometheus-compatible
  scrape output and OTLP export for Jaeger/collector deployments.
- Extensions: WebAssembly component plugins through host-granted capabilities.
- Themes: MiniJinja templates with a template modification system and strict
  context DTOs.

## Platform Boundary

Application code should remain portable across Linux, macOS, and Windows.
Linux-only assumptions belong in container images, rootless Podman scripts, or
clearly documented smoke-test helpers, not in the core binary. New runtime code
that needs OS-specific APIs must add matching target checks or tests. BSD
compatibility should not be broken casually, but it is not a release gate.

## Database Shape

Core tables are planned as `SCHEMAFULL` SurrealDB tables:

- `user`: account identity, security state, reputation, trust level.
- `session`: opaque session token hash, user, expiry, revocation state.
- `category`: nested forum tree and per-category settings.
- `topic`: thread metadata, state, counters, and type.
- `post`: raw content, sanitized HTML, edit metadata, moderation state.
- `role`: capability bundles.
- `audit_log`: append-only security and moderation events.

Core graph edges:

- `HAS_ROLE`: user to role.
- `MODERATES`: user to category.
- `POSTED`: user to post.
- `BELONGS_TO`: post to topic and topic to category.
- `READ`: user read state for topic/post.
- `WATCHES`: user subscriptions.

## Security Boundaries

- Rust service validates request shape before business logic.
- Business services enforce actor intent and capability checks.
- SurrealDB schema and permissions provide a second boundary.
- Plugin code crosses a WASM interface only and receives explicit input.
- Theme code renders explicit DTOs only and is backed by CSP.

## Fluxheim Compatibility

Mythenheim must work behind Fluxheim as a normal upstream service:

- bind to localhost or an internal container network;
- use `https://mythenheim.eu` as the production public origin;
- allow `https://dev.mythenheim.eu` for local development on this machine;
- respect `X-Forwarded-For`, `X-Forwarded-Proto`, and `Host` only from trusted
  proxy CIDRs;
- expose `/healthz`;
- keep OpenTelemetry trace context intact when Fluxheim forwards `traceparent`;
- avoid assuming TLS terminates in the app;
- support clean startup validation through `--check-config`.
