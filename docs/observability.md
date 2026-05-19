# Observability Plan

Mythenheim must support OpenTelemetry before `1.0.0`. The goal is operational
debuggability without leaking forum content, account secrets, cookies, or
unbounded user-controlled labels.

## Targets

- Prometheus-compatible metrics endpoint for local scraping.
- OpenTelemetry traces for HTTP requests, database calls, background jobs,
  moderation actions, plugin hooks, and template rendering.
- OTLP export that can be pointed at a local collector or Jaeger deployment.
- Trace-context propagation behind Fluxheim, preserving incoming `traceparent`
  and emitting child spans from Mythenheim.

## Privacy And Cardinality Rules

- Do not label metrics with usernames, emails, IP addresses, topic titles,
  post content, slugs, query strings, or raw user-agent values.
- Use route templates, status codes, bounded outcome names, and coarse module
  labels.
- Redact cookies, authorization headers, session IDs, CSRF tokens, passkey
  challenges, and plugin secrets from spans and logs.
- Treat plugin and theme names as bounded only after manifest validation.

## Planned Smoke Tests

- Prometheus scrape test against the local Podman Prometheus instance.
- Jaeger/OTLP trace test against the local Podman Jaeger or collector endpoint.
- Fluxheim-to-Mythenheim trace propagation test proving the same request
  carries a parent/child relationship across the proxy boundary.
- Negative tests that malicious path/query/header content does not become
  high-cardinality labels or span attributes.
