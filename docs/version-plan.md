# Version Plan

Mythenheim uses SemVer with Fluxheim-style release gates. A feature is not done
because it exists in code; it is done when it has tests, docs, config
validation, security review notes, and a smoke path.

## Version Rules

- `0.x`: incubator releases. Schema and APIs may change.
- `1.0.x`: stable forum-core bugfixes only.
- `1.x.0`: stable additive modules.
- `2.0.0`: breaking API, schema, plugin, or security-boundary changes.

Security fixes should be backported to the latest stable minor when practical.

## Release Ladder

### 0.10.0 - Repository And Safety Baseline

Scope:

- Rust 1.95 toolchain pin.
- EUPL-1.2 license and plugin/theme notice.
- Axum health service.
- Config parser and `--check-config`.
- Safe Markdown rendering primitive with XSS tests.
- Capability string validation.
- GitHub CI, check script, release metadata validation, doc link validation.
- Rootless Podman SurrealDB test helper.
- Fluxheim proxy notes and Wolfi reverse-proxy smoke fixture.

Done when:

- `scripts/checks.sh` passes.
- `cargo deny check` and `cargo audit` are runnable in CI.
- `scripts/smoke_local.sh` passes.
- Fluxheim Wolfi can proxy `Host: mythenheim.eu` to Mythenheim `/healthz`.

### 0.11.0 - SurrealDB Schema And Migrations Preview

Scope:

- SurrealDB migration definitions and CLI validation.
- Rust SDK admission remains deferred until license metadata and selected
  feature flags pass dependency policy.
- Schema bootstrap for `user`, `role`, `category`, `topic`, `post`,
  `session`, and `audit_log`.
- Migration runner with idempotency tests.
- Podman SurrealDB integration test script using random ports.

Done when:

- Unit tests cover schema generation and migration ordering.
- Integration smoke creates a temporary namespace/database in SurrealDB.
- Failed migrations cannot partially mark themselves complete.

### 0.12.0 - Accounts And Opaque Sessions Preview

Scope:

- Registration and login.
- Argon2id password hashing with current OWASP-tuned parameters.
- Opaque session tokens stored hashed server-side.
- Secure cookie settings and logout revocation.
- Account lockout and login rate-limit hooks.

Done when:

- Password hash tests verify successful and failed verification.
- Session revocation tests prove old cookies stop working.
- Auth endpoints reject malformed JSON and oversized bodies.

### 0.13.0 - Category, Topic, And Post Core

Scope:

- Category tree.
- Topic create/read/list.
- Post create/read/edit.
- Markdown to sanitized HTML persistence.
- Slugs, pagination, soft delete primitives.

Done when:

- XSS fixture tests cover Markdown, raw HTML, links, and images.
- Permission tests cover public/private category reads.
- API smoke test creates a category, topic, and reply.

### 0.14.0 - RBAC, ABAC, And Trust Levels

Scope:

- Capability resolver.
- Global roles and category-scoped moderator roles.
- Ownership checks such as `post.edit.own`.
- Trust levels TL0 through TL3.
- Role assignment escalation prevention.

Done when:

- Tests prove actors cannot grant capabilities they do not hold.
- Category-scoped moderation does not leak across categories.
- Trust promotion/demotion is deterministic and audited.

### 0.15.0 - Moderation Queue And Audit Log

Scope:

- Report queue.
- Approval queue.
- Warnings and warning points.
- Mute, ban, and shadowban state.
- Append-only audit events for every staff action.

Done when:

- Shadowbanned users see their own posts while others do not.
- Audit tests prove previous state is captured.
- Warning threshold tests trigger automatic mutes/bans.

### 0.16.0 - Search, Read State, Notifications

Scope:

- Full-text search indexes.
- Precise read/unread state.
- Watched topics/forums/tags.
- Mentions and notification records.
- WebSocket notification delivery preview.

Done when:

- Read-state tests cover new reply and mark-read flows.
- Search tests cover permissions and deleted content.
- WebSocket smoke test receives a mention notification.

### 0.17.0 - Attachments, Editor, And Anti-Abuse

Scope:

- Attachment upload metadata and limits.
- MIME and size validation.
- Link/image restrictions for low trust users.
- Keyword/regex filters.
- Rate limiting by IP and user.

Done when:

- Upload tests reject executable and mislabeled files.
- TL0 link/image restrictions are enforced.
- Rate-limit tests cover login and post creation.

### 0.18.0 - Admin And Moderator Interfaces API

Scope:

- Admin settings API.
- Moderator dashboard API.
- Custom moderation macros.
- Delayed moderation jobs.
- Metrics and health detail endpoints.
- OpenTelemetry trace propagation and service spans.
- Prometheus-compatible metrics endpoint.
- OTLP export config compatible with Jaeger or an OpenTelemetry Collector.

Done when:

- Config validation protects unsafe settings.
- Moderation macro tests prove transactional action behavior.
- Job tests prove delayed moderation executes once.
- Prometheus scrape smoke verifies bounded-label metrics.
- Jaeger/OTLP smoke verifies request traces from Fluxheim to Mythenheim.

### 0.19.0 - Themes And Server Rendering Preview

Scope:

- MiniJinja rendering.
- Theme inheritance and style properties.
- Template modification system.
- CSP nonce/reporting integration.
- Cached compiled templates.

Done when:

- Template tests reject unsafe function/database access.
- Conflicting template modifications are deterministic.
- CSP tests prove inline scripts without nonce are blocked by policy output.

### 0.20.0 - WASM Plugin Preview

Scope:

- Versioned WIT interface.
- Hook manager.
- Plugin manifest validation.
- Host capability grants.
- Plugin timeout, memory limit, and failure isolation.

Done when:

- Malicious fixture plugin cannot access filesystem/network/database.
- Panic/trap disables only the plugin, not Mythenheim.
- Hook ordering and mutation tests are deterministic.

### 0.21.0 - Importers And Migration Tools

Scope:

- Import framework for legacy forum exports.
- User, category, topic, post, attachment import mapping.
- Dry-run validation and import reports.

Done when:

- Fixture imports preserve post authorship and timestamps.
- Dry-run produces no writes.
- Permission mapping gaps are reported clearly.

### 1.0.0 - Stable Forum Core

Stable scope:

- Accounts, opaque sessions, password login.
- Category/topic/post core.
- Sanitized Markdown content.
- RBAC/ABAC permissions.
- Trust levels.
- Moderation queues, warnings, bans, shadowbans.
- Audit log.
- Search, read state, watched content, notifications.
- Attachments with strict validation.
- Rootless Podman deployment and direct compiled binary deployment.
- Fluxheim reverse-proxy compatibility.
- OpenTelemetry tracing and metrics with Prometheus and Jaeger/OTLP smoke tests.

Done when:

- Full release checklist passes.
- Podman and direct binary smoke tests pass.
- SurrealDB integration tests pass from a random rootless port.
- Fluxheim Wolfi reverse-proxy smoke passes for `mythenheim.eu` and
  `dev.mythenheim.eu`.
- Observability smoke proves Prometheus metrics and Jaeger-visible traces.
- Security review has no unresolved high or critical issues.
- Upgrade path from every `0.1x` schema preview is documented or explicitly
  rejected with migration guidance.

## Post-1.0 Stable Additions

### 1.1.0 - Stable Themes

Stabilize theme inheritance, style properties, TMS, CSP, and server rendering.

### 1.2.0 - Stable WASM Plugins

Stabilize plugin APIs, host grants, compatibility checks, and plugin admin UI.

### 1.3.0 - SSO And Passkeys

Stabilize WebAuthn/passkeys and OIDC SSO with account-linking tests.

### 1.4.0 - Federation And Feeds

Stabilize RSS/Atom, webhooks, and ActivityPub-compatible federation after abuse
controls are ready.

### 1.5.0 - Community Suite

Add clubs/groups, galleries, events, and richer profile/community mechanics.

### 1.6.0 - Moderation Intelligence

Add moderation replay mode, workload balancing, and safety budget dashboards.

Done when:

- Replay tests prove simulated moderation actions do not mutate production
  state.
- Queue-routing tests cover language, category, severity, and conflict rules.
- Metrics tests keep labels bounded and privacy-safe.

### 1.7.0 - Verifiable Governance

Add hash-linked audit exports, per-topic governance modes, proposal voting,
expert-only replies, slow mode, and consensus summaries.

Done when:

- Audit-chain verification survives backup/export/import round trips.
- Topic-mode permission tests cover every mode.
- Vote and summary tests prevent duplicate or unauthorized actions.

### 1.8.0 - User Data Portability

Add user-owned content packages for posts, attachments, bookmarks, profile data,
and redaction-aware exports.

Done when:

- Export tests prove private/deleted/third-party data boundaries are respected.
- Redaction tests prove secrets and other users' private data are excluded.
- Import tests can restore a user-owned archive into a test namespace.

### 1.9.0 - Extension Safety Tooling

Add plugin capability simulation and theme security linting before production
activation.

Done when:

- Plugin simulation tests prove requested grants match actual host access.
- Theme lint tests catch CSP conflicts, unsafe inline behavior, accessibility
  regressions, and layout overflow.
- Failed checks block activation without breaking the currently active theme or
  plugin set.

### 1.10.0 - Community Health Analytics

Add privacy-preserving community health metrics for response time, unanswered
topics, moderation delay, newcomer success, and content lifecycle transparency.

Done when:

- Analytics tests prove no usernames, emails, IPs, topic titles, post content,
  or raw user agents become metric labels.
- Lifecycle tests show pending/hidden/rate-limited/edit states to users without
  exposing anti-spam internals.
