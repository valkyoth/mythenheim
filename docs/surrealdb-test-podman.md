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

Security rules:

- bind only to `127.0.0.1`;
- use a throwaway namespace/database for tests;
- never reuse production credentials;
- delete test containers after use.
