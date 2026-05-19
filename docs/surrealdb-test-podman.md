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
SurrealDB container CLI instead of adding the Rust SDK crate.

The official SurrealDB license FAQ says SDKs and libraries are released under
Apache-2.0 or MIT, and that the BSL restriction on core database code is about
offering SurrealDB commercially as DBaaS. That is compatible with Mythenheim's
intended self-hosted forum use. The Rust SDK crate still needs a separate
security gate before admission: enabling the current latest `surrealdb` crate
for Rust pulls `rsa` through `jsonwebtoken` in `surrealdb-core`, and RustSec
currently reports `RUSTSEC-2023-0071` with no safe upgrade. Under Mythenheim's
security policy, that blocks committing the SDK dependency until the upstream
graph no longer contains the vulnerable RSA path or we can prove the path is
unreachable and accept the residual risk explicitly.

References:

- [SurrealDB license FAQ](https://surrealdb.com/license)
- [SurrealDB Rust SDK docs](https://surrealdb.com/docs/languages/rust/overview)
- [RustSec RUSTSEC-2023-0071](https://rustsec.org/advisories/RUSTSEC-2023-0071)

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
