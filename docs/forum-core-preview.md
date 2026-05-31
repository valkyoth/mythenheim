# Forum Core Preview

The first forum-core slice is an in-memory API preview for the `0.13.0`
milestone. It proves the request/response shape, validation, and content
sanitization before SurrealDB persistence is wired in.

## Routes

- `GET /api/v1/categories`
- `POST /api/v1/categories`
- `GET /api/v1/categories/tree`
- `GET /api/v1/categories/{category_id}/topics`
- `POST /api/v1/categories/{category_id}/topics`
- `GET /api/v1/topics/{topic_id}`
- `DELETE /api/v1/topics/{topic_id}`
- `GET /api/v1/posts/{post_id}`
- `POST /api/v1/topics/{topic_id}/posts`
- `PATCH /api/v1/posts/{post_id}`
- `DELETE /api/v1/posts/{post_id}`

Write routes require the current opaque session cookie. Public categories and
topics can be read anonymously. Private categories and their topics require a
valid session for reads until the RBAC/ABAC milestone adds granular category
capabilities.

## Behavior

- Category and topic slugs are generated from titles and made unique.
- Categories can be public or private and can have parents.
- Category reads are available as flat lists and nested trees.
- Topic creation creates the first post.
- Topic lists accept `page` and `page_size` query parameters.
- Replies increment topic `reply_count`.
- Posts store both raw Markdown and sanitized HTML.
- Posts can be read directly or through topic detail.
- Post edits re-render sanitized HTML and increment `revision`.
- Deleting a reply soft-deletes that post and decrements `reply_count`.
- Deleting a topic, or deleting its first post, soft-deletes the topic and its
  posts.
- Moderation-aware read paths can hide content from shadowbanned authors while
  leaving it visible to the author.
- Raw HTML events are dropped before rendering, then generated HTML is passed
  through `ammonia`.
- Empty titles, empty post content, NUL bytes, and oversized post bodies are
  rejected.
- Owner-only edit/delete checks are temporary 0.13 primitives. The 0.14
  RBAC/ABAC milestone layers capability-aware authorization over these
  primitives.

## Current Limits

This preview intentionally stores forum data in memory. The persistent version
must move the same API behavior to SurrealDB and preserve the current tests,
including XSS fixtures and the local smoke flow.
