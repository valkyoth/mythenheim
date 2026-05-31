# Permissions Preview

The `0.14.0` permission slice starts as a pure Rust policy layer. It is not yet
wired into every HTTP route, but it defines the behavior that the forum
services will use when RBAC/ABAC replaces the temporary owner-only checks from
the `0.13.0` forum preview.

## Model

- `Capability` validates granular permission strings such as `post.edit.own`.
- `Role` groups capabilities for global assignments.
- `ScopedRole` applies a role to one category only.
- `ActorPermissions` combines trust-level grants, global roles, and scoped
  category roles.
- `PermissionContext` carries the actor, optional owner, and optional category
  being checked.

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

## Current Limits

The resolver is in-memory and service-local. Persistent role assignment,
category inheritance, audited permission changes, and HTTP middleware wiring
come in follow-up `0.14.0` passes.
