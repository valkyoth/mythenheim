# Platform Support

Mythenheim supports the compiled application binary on Linux, macOS, and
Windows. The container image remains Linux-only by design, but the forum server
itself should avoid Linux-specific runtime assumptions unless a feature is
explicitly documented as container-only or development-only. BSD is a
best-effort source portability goal, not a release-blocking target.

## Target Policy

- Linux is the primary production and container target.
- Windows is a supported binary target for direct compiled-binary deployments.
- macOS is a supported binary target for direct compiled-binary deployments and
  local development.
- BSD is best-effort. Avoid unnecessary breakage, but do not block releases on
  BSD checks unless a future release explicitly promotes it.

Portable binary code should use Rust standard-library abstractions such as
`PathBuf`, `SocketAddr`, and Tokio networking instead of OS-specific APIs.
Introducing `std::os::*`, target-specific `cfg` gates, Unix sockets, shell-only
runtime behavior, or hardcoded POSIX paths in application code requires a
documented reason and a CI/test strategy.

## CI Coverage

The main Linux CI job runs formatting, linting, tests, release metadata checks,
documentation checks, migration validation, local smoke tests, dependency
policy, and RustSec advisory checks.

The binary portability CI job runs:

- native `cargo test --locked` on Linux;
- native `cargo test --locked` on Windows;
- native `cargo test --locked` on macOS.

Native release artifacts should be produced with
`scripts/build_release_binary.py` on the target operating system. GitHub Actions
can produce Linux, Windows, and macOS artifacts through the manual release
binary workflow.

## Linux-Only Tooling

These project areas are intentionally Linux-oriented:

- rootless Podman container operation;
- SurrealDB rootless Podman smoke scripts;
- Fluxheim Wolfi reverse-proxy smoke scripts;
- shell helper scripts under `scripts/`.

Those checks remain required for Linux/container release confidence, but they
must not become prerequisites for running the Mythenheim binary on Windows,
or macOS.
