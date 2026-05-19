# Forum Core Preview

The first forum-core slice is an in-memory API preview for the `0.13.0`
milestone. It proves the request/response shape, validation, and content
sanitization before SurrealDB persistence is wired in.

## Routes

- `GET /api/v1/categories`
- `POST /api/v1/categories`
- `GET /api/v1/categories/{category_id}/topics`
- `POST /api/v1/categories/{category_id}/topics`
- `GET /api/v1/topics/{topic_id}`
- `POST /api/v1/topics/{topic_id}/posts`

Write routes require the current opaque session cookie. Read routes are public
for now. Category administration permissions move into the RBAC/ABAC milestone.

## Behavior

- Category and topic slugs are generated from titles and made unique.
- Topic creation creates the first post.
- Replies increment topic `reply_count`.
- Posts store both raw Markdown and sanitized HTML.
- Raw HTML events are dropped before rendering, then generated HTML is passed
  through `ammonia`.
- Empty titles, empty post content, NUL bytes, and oversized post bodies are
  rejected.

## Current Limits

This preview intentionally stores forum data in memory. The persistent version
must move the same API behavior to SurrealDB and preserve the current tests,
including XSS fixtures and the local smoke flow.
