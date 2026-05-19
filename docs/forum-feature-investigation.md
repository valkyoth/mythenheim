# Forum Feature Investigation

This first-commit feature map is based on a one-time review of established
forum platform functionality on 2026-05-19. The project documentation keeps the
result vendor-neutral: Mythenheim tracks capability coverage, not product names.

## Parity Coverage

Mythenheim should treat these as product modules, not one large feature blob.
Each module has an owner version in the release plan.

| Capability area | Required coverage | Version target |
| --- | --- | --- |
| Repository, build, security baseline | Rust toolchain pin, license, checks, CI, release gates, rootless test helpers, Fluxheim proxy smoke | `0.10.0` |
| Database foundation | Strict schema, migrations, graph relationships, idempotent bootstrap, random-port SurrealDB tests | `0.11.0` |
| Accounts and sessions | Registration, login, secure cookies, opaque revocable sessions, password hashing, lockout hooks | `0.12.0` |
| Forum hierarchy | Categories, nested forums, topics, replies, slugs, pagination, soft deletes | `0.13.0` |
| Authoring and rendering | Markdown, sanitized HTML, edit windows, drafts, previews, stored raw/source content | `0.13.0`, `0.17.0` |
| Topic operations | Pin, lock, move, merge, split, restore, per-topic state, moderator notes | `0.13.0`, `0.15.0` |
| Permissions | Global roles, category roles, ownership checks, temporary grants, trust levels, escalation prevention | `0.14.0` |
| Moderation | Report queue, approval queue, warnings, points, mutes, bans, shadowbans, delayed moderation, moderation macros | `0.15.0`, `0.18.0` |
| Auditability | Append-only staff actions, previous-state capture, role-change records, moderation accountability | `0.15.0` |
| Search and discovery | Full-text search, permission-filtered results, tags, unread/read state, watched forums/topics/tags | `0.16.0` |
| Notifications | Mentions, watched-content alerts, realtime delivery preview, email hooks later | `0.16.0` |
| Attachments and media | Upload metadata, size limits, MIME validation, image rules, trust-gated links and embeds | `0.17.0` |
| Anti-abuse | Rate limits, low-trust restrictions, keyword/regex filters, spam scoring, privacy-preserving IP/device signals | `0.17.0` |
| Admin operations | Settings API, moderator dashboard API, health detail, logs, import/export, backup/restore hooks | `0.18.0`, `0.21.0` |
| Observability | Prometheus-compatible metrics, OpenTelemetry traces, OTLP export, Fluxheim trace propagation | `0.18.0`, `1.0.0` |
| Themes | Template rendering, inheritance, style properties, template modifications, CSP, cached compiled templates | `0.19.0`, `1.1.0` |
| Extensions | WASM plugin hooks, manifest validation, host grants, timeouts, memory limits, compatibility checks | `0.20.0`, `1.2.0` |
| Import and migration | Dry-run importer framework, users, categories, topics, posts, attachments, permission-gap reporting | `0.21.0` |
| Stable core | Accounts, forum engine, permissions, moderation, audit, search, notifications, attachments, deployment, observability | `1.0.0` |
| Identity integrations | Passkeys, OIDC SSO, account linking, recovery flows | `1.3.0` |
| Federation and feeds | RSS/Atom, webhooks, ActivityPub-compatible federation after abuse controls are stable | `1.4.0` |
| Community suite | Clubs/groups, galleries, events, richer profiles, member spaces | `1.5.0` |

## Mythenheim Ideas Beyond Parity

These are first-party ideas that fit Mythenheim's architecture and should be
considered after the core is safe:

- Moderation replay mode: staff can preview how a rule, trust-level change, or
  anti-abuse filter would have affected recent content before enabling it.
- Cryptographic audit chain: hash-linked audit events with exportable proofs so
  staff actions can be verified after backup/restore or incident response.
- Per-topic governance modes: a topic can opt into normal discussion,
  question/answer, proposal voting, slow mode, expert-only replies, or
  consensus summary without changing the whole forum.
- User-owned content packages: members can export their posts, attachments,
  bookmarks, and profile data in a portable archive with redaction controls.
- Safety budget dashboard: admins see which rate limits, queues, and trust
  restrictions are doing useful work instead of silently accumulating rules.
- Plugin capability simulator: operators can test what a WASM plugin is allowed
  to read, write, and emit before installing it on production data.
- Theme security linter: theme/template changes get checked for CSP conflicts,
  unsafe inline behavior, accessibility regressions, and layout overflow before
  activation.
- Moderator workload balancing: report queues can be routed by language,
  category, severity, conflict-of-interest rules, and staff availability.
- Transparent content lifecycle: users can see why their content is pending,
  hidden, rate-limited, or edited, unless doing so would weaken spam handling.
- Privacy-preserving community health metrics: trend data for response time,
  unanswered topics, moderation delay, and newcomer success without exposing
  individual behavior.

## Design Consequence

The version plan intentionally delays plugin execution, federation, and rich
community-suite features until the core permission, moderation, audit,
observability, and content-safety model has already been proven by tests.
