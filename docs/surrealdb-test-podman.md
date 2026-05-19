# Rootless SurrealDB Testing

Mythenheim integration tests must not assume a fixed SurrealDB port. The helper
script starts SurrealDB through rootless Podman and asks Podman to allocate a
random localhost port.

```sh
scripts/start_surrealdb_test.sh
```

Example output:

```sh
MYTHENHEIM_SURREALDB_CONTAINER=mythenheim-surrealdb-test-12345
MYTHENHEIM_DATABASE_ENDPOINT=ws://127.0.0.1:41033
Stop with: podman rm -f mythenheim-surrealdb-test-12345
```

The container uses SurrealDB memory storage for tests. Persistent development
databases should use a named volume and a separate script once schema migration
tests exist.

The `0.11.0` migration preview deliberately uses generated SurrealQL and the
SurrealDB container CLI instead of adding the Rust SDK crate. Current crate
metadata for the latest beta SDK reports an unknown license to Cargo tooling,
so Mythenheim keeps `cargo-deny` strict until the dependency can be admitted
with clear license metadata and a reviewed feature set.

Security rules:

- bind only to `127.0.0.1`;
- use a throwaway namespace/database for tests;
- never reuse production credentials;
- delete test containers after use.

## Migration Smoke

Use the migration smoke when changing schema definitions:

```sh
scripts/smoke_surrealdb_migrations.sh
```

The smoke uses a rootless SurrealDB container and applies the generated schema
twice to verify idempotency.
