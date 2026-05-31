# Release Checklist

Every release must have a clear finish line.

## Required For All Releases

- `scripts/checks.sh` passes.
- `scripts/smoke_local.sh` passes.
- `cargo deny check` passes or documented advisories are explicitly accepted.
- `cargo audit` passes or documented advisories are explicitly accepted.
- Binary portability CI passes for Linux, Windows, macOS, and the FreeBSD
  target check.
- Native release binary artifacts are built with `scripts/build_release_binary.py`
  or explicitly marked not applicable for the release.
- Version plan entry exists.
- Release notes describe stable, beta, and known-gap behavior truthfully.
- Security-sensitive changes include tests.
- Fluxheim compatibility is either tested through the Wolfi smoke fixture or
  explicitly marked not applicable for the release.

## Required Before 1.0.0

- Rootless Podman SurrealDB integration tests pass on random ports.
- Direct compiled binary smoke test passes.
- Linux, Windows, macOS, and BSD direct-binary support is either verified or
  accurately documented with known gaps.
- Container smoke test passes as a non-root user.
- Fluxheim Wolfi proxy smoke passes for `mythenheim.eu` and `dev.mythenheim.eu`.
- Permission and moderation tests cover escalation, shadowban visibility,
  audit logging, and deleted/private content access.
- Content tests cover Markdown, BBCode compatibility, attachments, stored HTML,
  and browser-facing CSP.
- Observability smoke verifies Prometheus metrics and OpenTelemetry traces
  reaching the local Jaeger/OTLP stack.
- Dependency review has no unresolved high or critical issues.
