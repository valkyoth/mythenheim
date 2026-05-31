# Moderation Preview

The `0.15.0` moderation slice starts as an in-memory service so the behavior
can be tested before it moves behind SurrealDB transactions and staff-facing
APIs.

## Model

- `Report` stores user-submitted reports in an open moderation queue.
- `ApprovalItem` stores content that must be manually approved before normal
  visibility.
- `Warning` stores warning points issued by staff.
- `UserModerationState` tracks active warning points, mute state, ban state,
  and shadowban state.
- `AuditEvent` records staff and automated moderation actions with previous
  and new user moderation state where the action changes a user.

## Current Rules

- Moderation reasons are required, length-limited, and reject NUL bytes.
- Zero-point warnings are rejected.
- Warning points are summed from active warnings.
- Users are automatically muted at `5` active warning points.
- Users are automatically banned at `10` active warning points.
- Shadowbanned authors can see their own topics and posts.
- Other authenticated users and anonymous users cannot see content authored by
  shadowbanned users through topic, topic-list, or direct-post reads.
- Audit events are append-only in the preview service API.

## Current Limits

The service is still in-memory. Persistent queues, staff dashboard routes,
permission-gated moderation APIs, warning expiration, transactional moderation
macros, and delayed moderation jobs remain follow-up `0.15.0` work.
