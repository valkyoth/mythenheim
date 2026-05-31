# Permissions Preview

The `0.14.0` permission slice starts as a pure Rust policy layer and is wired
into the preview forum HTTP write paths. It defines the behavior that the forum
services will use when persistent RBAC/ABAC replaces the temporary in-memory
preview roles.

## Model

- `Capability` validates granular permission strings such as `post.edit.own`.
- `Role` groups capabilities for global assignments.
- `ScopedRole` applies a role to one category only.
- `ActorPermissions` combines trust-level grants, global roles, and scoped
  category roles.
- `PermissionContext` carries the actor, optional owner, and optional category
  being checked.
- `PermissionService` stores preview roles, default roles, global user role
  assignments, and category-scoped user role assignments.

## Current Rules

- `*.own` capabilities require the actor to own the resource.
- Matching `*.any` capabilities satisfy the corresponding own action.
- Category-scoped roles only apply to their configured category.
- Trust levels add deterministic baseline capabilities:
  - TL0/New Seed: public category read.
  - TL1/Wanderer: topic creation and replies.
  - TL2/Citizen: own post edit/delete and reactions.
  - TL3/Elder: trusted flags and veteran category reads.
- Role assignment is blocked when the actor does not already hold every
  capability included in the target role.
- Forum preview routes check capabilities before creating categories, creating
  topics, replying, editing posts, deleting posts, deleting topics, or reading
  private categories.
- Role assignment checks use the same resolver, so an actor cannot assign a
  global or category-scoped role containing capabilities they do not already
  hold.

## Current Limits

The resolver and role assignments are still in-memory. Persistent role
assignment, audited permission changes, category inheritance, and extractor or
middleware ergonomics come in follow-up `0.14.0` passes.
