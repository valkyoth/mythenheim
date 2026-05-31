# Release Binary Builds

Mythenheim release binaries are built natively on the operating system they
target. The helper script clones a clean copy of the repository, checks out the
requested release tag, installs the pinned Rust toolchain when requested,
builds with `cargo build --release --locked`, packages the binary, and prints
SHA256 values for release notes.

## Supported Native Builds

| Platform argument | Run on | Architectures | Package |
| --- | --- | --- | --- |
| `linux` | Linux | native `x86_64`, `aarch64`, ARM where Rust supports the host | `.tar.gz` |
| `macos` | macOS | native Intel and Apple Silicon | `.tar.gz` |
| `windows` | Windows | native `x86_64`, `aarch64`, ARM where Rust supports the host | `.zip` |

The script does native builds by default. Do not use it to imply that a Linux
host can produce official macOS or Windows artifacts. Run it on each target
operating system, then copy the SHA256 lines into the release notes.

Native ARM hosts work without special flags. The architecture is included in
the artifact name. For explicit Rust target triples, pass `--target`, for
example `aarch64-unknown-linux-gnu`. Cross-target builds still require the
correct linker and system libraries on the build host.

The script clones the repository before building. Release artifacts must be
built from an exact Git tag that matches the Cargo package version, for example
`v0.12.0` for package version `0.12.0`. Use `--allow-untagged` only for local
validation builds that will not be uploaded to a release.

Artifact names use the package version, operating-system label, and
architecture:

```text
mythenheim-0.12.0-linux-x86_64.tar.gz
mythenheim-0.12.0-macos-x86_64.tar.gz
mythenheim-0.12.0-windows11-x86_64.zip
mythenheim-0.12.0-windowsserver2026-x86_64.zip
```

Use `--os-label` when an artifact should name a specific supported operating
system variant. Use lowercase labels such as `windows11`,
or `windowsserver2026`.

## Examples

Linux:

```sh
python3 scripts/build_release_binary.py linux --ref v0.12.0 --install-prereqs
```

Linux ARM target from a prepared Linux build host:

```sh
python3 scripts/build_release_binary.py linux --ref v0.12.0 \
  --target aarch64-unknown-linux-gnu --install-prereqs
```

macOS:

```sh
python3 scripts/build_release_binary.py macos --ref v0.12.0 --install-prereqs
```

Apple Silicon macOS can run the same command natively. An explicit target such
as `aarch64-apple-darwin` is only needed when the build host is configured for
that cross-target.

Windows:

```powershell
py -3 scripts/build_release_binary.py windows --ref v0.12.0 --install-prereqs
```

Windows 11 label:

```powershell
py -3 scripts/build_release_binary.py windows --ref v0.12.0 --os-label windows11 --install-prereqs
```

Windows Server 2026 label:

```powershell
py -3 scripts/build_release_binary.py windows --ref v0.12.0 --os-label windowsserver2026 --install-prereqs
```

`git` and Python must already be available so the script can run and clone the
repository. `--install-prereqs` installs Rust through `rustup` when Cargo is not
already available; package-manager setup for Git and Python remains an operator
bootstrap step.

## Local Validation

For an untagged local validation build from the current checkout:

```sh
python3 scripts/build_release_binary.py linux \
  --repo . \
  --ref HEAD \
  --allow-untagged \
  --out-dir target/release-binaries
```

This is useful for testing packaging changes, but it must not be uploaded as an
official release artifact.

## Output

Artifacts are written to:

```text
target/release-binaries/
```

The script prints two release-note lines:

- packaged artifact SHA256;
- raw binary SHA256.

Use the packaged artifact SHA256 for GitHub release assets. Keep the raw binary
SHA256 as additional evidence in the versioned release notes.
