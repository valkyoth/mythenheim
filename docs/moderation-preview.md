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

## Routes

- `POST /api/v1/posts/{post_id}/reports`
- `GET /api/v1/moderation/reports`
- `POST /api/v1/moderation/reports/{report_id}/resolve`
- `GET /api/v1/moderation/approvals`
- `POST /api/v1/moderation/approvals/{approval_id}/resolve`
- `POST /api/v1/moderation/warnings`
- `POST /api/v1/moderation/warnings/{warning_id}/expire`
- `POST /api/v1/moderation/users/{user_id}/shadowban`
- `GET /api/v1/moderation/audit`

Reporting a post requires an authenticated user and the post must be visible to
that user. Queue reads require `moderation.queue.read`, queue resolution
requires `moderation.queue.write`, warning issuance and expiration require
`user.warn`, shadowban changes require `user.shadowban`, and audit reads
require `audit.read`.

## Current Rules

- Moderation reasons are required, length-limited, and reject NUL bytes.
- Zero-point warnings are rejected.
- Warning points are summed from active warnings.
- Users are automatically muted at `5` active warning points.
- Users are automatically banned at `10` active warning points.
- Expiring a warning marks it inactive, recomputes active warning points, and
  can automatically clear mute/ban state when thresholds are no longer met.
- Shadowbanned authors can see their own topics and posts.
- Other authenticated users and anonymous users cannot see content authored by
  shadowbanned users through topic, topic-list, or direct-post reads.
- Audit events are append-only in the preview service API.
- Resolving a report or approval item removes it from the open queue and writes
  a resolution audit event.
- Staff-facing preview routes are capability-gated before they call the
  moderation service.

## Current Limits

The service is still in-memory. Persistent queues, staff dashboard routes,
warning expiration, transactional moderation macros, and delayed moderation
jobs remain follow-up `0.15.0` work.
